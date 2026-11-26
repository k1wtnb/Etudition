//! 練習のコマンド（境界の約束 B0 の5章）。配線だけで、採点と記録は `core` と `engine` が持つ。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use etudition_core::expand::{Expansion, check_playable, expand};
use etudition_core::quarters::Quarters;
use etudition_core::rules::RULES_V0;
use etudition_core::score::read_score;
use etudition_core::timing::{Tempo, Timing, span_to_ns};
use etudition_engine::input::{MidiInput, StartError};
use etudition_engine::record::score_hash;
use etudition_engine::session::{PracticeSetup, RunMode, SessionOutcome, open_session};
use tauri::ipc::Channel;

use crate::view::{LiveEvent, ScoreView, TempoView};

/// 読み込んだ楽譜。
pub struct Loaded {
    /// 楽譜のファイルのバイト列（お手本のハッシュに使う）。
    pub bytes: Vec<u8>,
    /// お手本。
    pub expansion: Expansion,
}

/// アプリの状態（読み込んだ楽譜と、練習の回の終了の要求）。
#[derive(Default)]
pub struct AppState {
    loaded: Mutex<Option<Arc<Loaded>>>,
    stop: Mutex<Arc<AtomicBool>>,
}

/// 楽譜を読み、採点に使えるかを確かめる（B07、REQ-105）。
pub fn load(bytes: Vec<u8>) -> Result<(Loaded, String), String> {
    let loaded = read_score(&bytes).map_err(|e| format!("読めない楽譜: {e}"))?;
    let expansion = expand(&loaded.score);
    check_playable(&expansion).map_err(|issues| {
        let list: Vec<String> = issues
            .iter()
            .map(|i| match i.measure_index {
                Some(m) => format!("{}（小節 {}）: {}", i.element, m + 1, i.detail),
                None => format!("{}: {}", i.element, i.detail),
            })
            .collect();
        format!("採点に使えない楽譜：{}", list.join("、"))
    })?;
    Ok((Loaded { bytes, expansion }, loaded.xml))
}

/// 前カウントの拍の数（計画6章 B06）：1小節。1小節の拍が4拍より少ないときは2小節。
/// 拍は四分音符で数える（採点の仕様 v0 の SR-05）。複合拍子などの数え方は Q-29 で決める（それまでは仮）。
pub fn count_in_beats(e: &Expansion) -> u32 {
    // 弱起の小節（負の位置から始まる小節）は数えない。1小節目の長さは、次の小節の頭との差
    let starts: Vec<Quarters> = e
        .measures
        .iter()
        .map(|m| m.start)
        .filter(|s| *s >= Quarters::ZERO)
        .collect();
    let beats = starts
        .windows(2)
        .next()
        .and_then(|w| {
            let len = w[1] - w[0];
            u32::try_from(len.num() / len.den()).ok()
        })
        .filter(|&b| b > 0)
        .unwrap_or(4);
    if beats < 4 { beats * 2 } else { beats }
}

/// 練習のテンポと前カウントから、楽譜の位置 0 の時刻を決める（原点は前カウントの最初のクリック）。
pub fn timing_of(tempo: TempoView, e: &Expansion) -> Result<Timing, String> {
    let tempo = Tempo::new(tempo.num, tempo.den).ok_or("テンポが正でない")?;
    let beats = i64::from(count_in_beats(e));
    Ok(Timing {
        tempo,
        origin_ns: span_to_ns(&tempo, Quarters::new(beats, 1)),
    })
}

/// 1回の練習を、練習のスレッドで進め、通知を `send` に渡す。
pub fn run(
    loaded: &Loaded,
    timing: Timing,
    input: Box<dyn MidiInput + Send>,
    mode: RunMode,
    record_dir: PathBuf,
    stop: &AtomicBool,
    send: &dyn Fn(LiveEvent),
) -> Result<SessionOutcome, StartError> {
    let setup = PracticeSetup {
        expansion: loaded.expansion.clone(),
        score_hash: score_hash(&loaded.bytes),
        timing,
        count_in_beats: count_in_beats(&loaded.expansion),
        correction_ns: 0,
        record_dir,
        rules: &RULES_V0,
    };
    let open = open_session(setup, input)?;
    let notify = |ev| match LiveEvent::from_engine(ev) {
        Ok(live) => send(live),
        // 画面に渡せない時刻（約104日を超える）は、採点と記録には影響しない。画面の通知だけを落とす
        Err(e) => eprintln!("画面に渡せない通知: {e}"),
    };
    Ok(open.run(mode, &notify, stop))
}

/// この回の記録先（アプリのデータのフォルダの `runs/` の下。名前は始めた時刻のミリ秒）。
pub fn record_dir(app_data: &Path) -> PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    app_data.join("runs").join(format!("{ms}"))
}

/// 楽譜を読む。
#[tauri::command]
pub fn load_score(path: String, state: tauri::State<'_, AppState>) -> Result<ScoreView, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("{path} を開けない: {e}"))?;
    let (loaded, xml) = load(bytes)?;
    let view = ScoreView::new(xml, &loaded.expansion.sounding);
    *state.loaded.lock().map_err(|e| e.to_string())? = Some(Arc::new(loaded));
    Ok(view)
}

/// 練習を始める。実機の MIDI の入力は 1a-8 で足すので、今は「MIDI 機器がない」を返す（REQ-103）。
#[tauri::command]
pub fn start_practice(_tempo: TempoView, _events: Channel<LiveEvent>) -> Result<(), String> {
    Err(StartError::NoMidiDevice.to_string())
}

/// 練習を終える（使う人が閉じた回。記録しない。仕様書 10-1）。
#[tauri::command]
pub fn stop_practice(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .stop
        .lock()
        .map_err(|e| e.to_string())?
        .store(true, Ordering::Relaxed);
    Ok(())
}

/// 開発用のビルドだけ（cargo の feature `dev-feed`。REQ-106、AC-23）：MIDI ファイルを流し込んで練習の回を進める。
#[cfg(feature = "dev-feed")]
#[tauri::command]
pub fn feed_midi_file(
    path: String,
    tempo: TempoView,
    events: Channel<LiveEvent>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Manager;
    let loaded = state
        .loaded
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("先に楽譜を読む")?;
    let timing = timing_of(tempo, &loaded.expansion)?;
    let midi = std::fs::read(&path).map_err(|e| format!("{path} を開けない: {e}"))?;
    let input = etudition_engine::input::FileInput::new(&midi, &loaded.expansion, &timing)
        .map_err(|e| e.to_string())?;
    let dir = record_dir(&app.path().app_data_dir().map_err(|e| e.to_string())?);
    let stop = Arc::new(AtomicBool::new(false));
    *state.stop.lock().map_err(|e| e.to_string())? = Arc::clone(&stop);
    std::thread::spawn(move || {
        let send = |ev: LiveEvent| {
            let _ = events.send(ev);
        };
        if let Err(e) = run(
            &loaded,
            timing,
            Box::new(input),
            RunMode::Feed,
            dir,
            &stop,
            &send,
        ) {
            eprintln!("流し込みを始められない: {e}");
        }
    });
    Ok(())
}

/// 登録するコマンドの名前（呼び出しの試験と、開発用のビルドの切り分けの試験に使う）。
pub fn command_names() -> Vec<&'static str> {
    let mut names = vec![
        "p0_time_probe",
        "load_score",
        "start_practice",
        "stop_practice",
    ];
    if cfg!(feature = "dev-feed") {
        names.push("feed_midi_file");
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use etudition_engine::input::FileInput;

    const T1: &[u8] = include_bytes!("../../../fixtures/1a/t1.musicxml");
    const PICKUP: &[u8] = include_bytes!("../../../fixtures/s1/07-pickup.musicxml");

    /// 計画 2-3 の3：画面の呼び出し関数（commands.ts）が、登録するコマンドの名前で呼ぶ。
    #[test]
    fn ui_calls_every_command_by_its_name() {
        let ts = include_str!("../../ui/src/lib/commands.ts");
        let mut all = command_names();
        if !all.contains(&"feed_midi_file") {
            all.push("feed_midi_file");
        }
        for name in all {
            assert!(
                ts.contains(&format!("\"{name}\"")),
                "commands.ts が {name} を呼んでいない"
            );
        }
    }

    /// AC-23 の AI の部分：開発用のビルドでない設定では、流し込みのコマンドが含まれない。
    #[test]
    fn feed_command_only_in_dev_build() {
        assert_eq!(
            command_names().contains(&"feed_midi_file"),
            cfg!(feature = "dev-feed")
        );
    }

    #[test]
    fn count_in_follows_b06() {
        let (t1, _) = load(T1.to_vec()).unwrap();
        assert_eq!(count_in_beats(&t1.expansion), 4);
        // 弱起の小節は数えない（07 は 4/4）
        let (p, _) = load(PICKUP.to_vec()).unwrap();
        assert_eq!(count_in_beats(&p.expansion), 4);
        let t = timing_of(TempoView { num: 60, den: 1 }, &t1.expansion).unwrap();
        assert_eq!(t.origin_ns, 4_000_000_000);
    }

    #[test]
    fn unplayable_score_is_refused_with_reasons() {
        let xml = br#"<?xml version="1.0"?><score-partwise><part-list/><part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes><note><rest/><duration>4</duration></note></measure></part></score-partwise>"#;
        let err = load(xml.to_vec()).err().unwrap();
        assert!(err.contains("採点に使えない楽譜"), "{err}");
    }

    /// 1a-6 の結合（AI の部分）：流し込みを始めると、画面への通知（音ごとの判定と結果）が届く。時刻は number。
    #[test]
    fn feed_reaches_the_screen_events() {
        let (loaded, _) = load(T1.to_vec()).unwrap();
        let timing = timing_of(TempoView { num: 60, den: 1 }, &loaded.expansion).unwrap();
        let midi = etudition_test_midi(&loaded.expansion);
        let input = FileInput::new(&midi, &loaded.expansion, &timing).unwrap();
        let dir = std::env::temp_dir().join(format!("etudition-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let got = Mutex::new(Vec::new());
        let outcome = run(
            &loaded,
            timing,
            Box::new(input),
            RunMode::Feed,
            dir.join("run"),
            &AtomicBool::new(false),
            &|ev| got.lock().unwrap().push(ev),
        )
        .unwrap();
        assert!(matches!(outcome, SessionOutcome::Scored(_)));
        let got = got.into_inner().unwrap();
        assert!(matches!(got.first(), Some(LiveEvent::Started { .. })));
        assert!(got.iter().any(|e| matches!(e, LiveEvent::Marks { .. })));
        let Some(LiveEvent::Ended { result, record }) = got.last() else {
            panic!("終わりの通知が届くはず");
        };
        assert_eq!(result.passed, Some(true));
        assert_eq!(*record, crate::view::RecordView::Written);
        let json = serde_json::to_value(got.last().unwrap()).unwrap();
        assert_eq!(json["type"], "ended");
        assert_eq!(json["result"]["end_at"]["ns"], 16_000_000_000_i64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 楽譜どおりの MIDI（形式 0、960 tick／四分音符）。
    fn etudition_test_midi(e: &Expansion) -> Vec<u8> {
        use midly::num::{u4, u7, u15, u28};
        use midly::{
            Format, Header, MetaMessage, MidiMessage, Smf, Timing as T, TrackEvent, TrackEventKind,
        };
        let tick = |q: Quarters| q.num() * 960 / q.den();
        let mut evs: Vec<(i64, u8, u8, bool)> = Vec::new();
        for s in &e.sounding {
            evs.push((tick(s.start), 1, s.pitch as u8, true));
            evs.push((tick(s.start + s.duration), 0, s.pitch as u8, false));
        }
        evs.sort_unstable();
        let mut track = Vec::new();
        let mut prev = 0;
        for (t, _, key, on) in evs {
            let message = if on {
                MidiMessage::NoteOn {
                    key: u7::new(key),
                    vel: u7::new(100),
                }
            } else {
                MidiMessage::NoteOff {
                    key: u7::new(key),
                    vel: u7::new(0),
                }
            };
            track.push(TrackEvent {
                delta: u28::new(u32::try_from(t - prev).unwrap()),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message,
                },
            });
            prev = t;
        }
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        let smf = Smf {
            header: Header::new(Format::SingleTrack, T::Metrical(u15::new(960))),
            tracks: vec![track],
        };
        let mut out = Vec::new();
        smf.write_std(&mut out).unwrap();
        out
    }
}
