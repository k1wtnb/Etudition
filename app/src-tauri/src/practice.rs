//! 練習のコマンド（境界の約束 B0 の5章）。配線だけで、採点と記録は `core` と `engine` が持つ。
//!
//! 機器（入力）はコマンドのスレッドで1回だけ開き、開けなければコマンドの誤りとして画面に返す（B0 の4章）。
//! 開けたら練習のスレッドを作って回を進め、通知は Channel で画面へ送る。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use etudition_core::expand::{Expansion, check_playable, expand};
use etudition_core::quarters::Quarters;
use etudition_core::rules::RULES_V0;
use etudition_core::score::{Score, read_score};
use etudition_core::timing::{Tempo, Timing, span_to_ns};
use etudition_engine::input::{MidiInput, StartError};
use etudition_engine::record::score_hash;
use etudition_engine::session::{OpenSession, PracticeSetup, RunMode, open_session};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime};

use crate::view::{LiveEvent, ScoreView, TempoView};

/// 読み込んだ楽譜。
pub struct Loaded {
    /// 楽譜のファイルのバイト列（お手本のハッシュに使う）。
    pub bytes: Vec<u8>,
    /// お手本。
    pub expansion: Expansion,
    /// 前カウントの拍の数（[`count_in_beats`]）。
    pub count_in_beats: u32,
}

/// アプリの状態。
#[derive(Default)]
pub struct AppState {
    /// 読み込んだ楽譜。
    loaded: Mutex<Option<Arc<Loaded>>>,
    /// 進んでいる練習の回の終了の要求（回がなければ `None`）。
    running: Mutex<Option<Arc<AtomicBool>>>,
    /// 記録の置き場所（`None` ならアプリのデータのフォルダ。試験で替える）。
    record_root: Mutex<Option<PathBuf>>,
}

impl AppState {
    /// 記録の置き場所を替える（試験用。`None` ならアプリのデータのフォルダ）。
    pub fn set_record_root(&self, root: Option<PathBuf>) -> Result<(), String> {
        *lock(&self.record_root)? = root;
        Ok(())
    }

    /// 記録の置き場所（替えていなければ `app_data`）。
    pub fn record_root_or(
        &self,
        app_data: impl FnOnce() -> Result<PathBuf, String>,
    ) -> Result<PathBuf, String> {
        match lock(&self.record_root)?.clone() {
            Some(r) => Ok(r),
            None => app_data(),
        }
    }
}

/// 楽譜を読み、採点に使えるかを確かめる（B07、REQ-105）。戻り値の2つ目は画面に渡す楽譜。
pub fn load(bytes: Vec<u8>) -> Result<(Loaded, ScoreView), String> {
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
    let count_in_beats = count_in_beats(&loaded.score);
    let view = ScoreView::new(loaded.xml, &loaded.score.notes, &expansion.sounding)?;
    Ok((
        Loaded {
            bytes,
            expansion,
            count_in_beats,
        },
        view,
    ))
}

/// 前カウントの拍の数：どの拍子でも1小節（Q-29。人間の回答 2026-11-26）。
/// 拍は四分音符で数え（採点の仕様 v0 の SR-05）、1小節の長さは弱起でない最初の小節の拍子から取る（書かれた音の長さではない）。
/// 分母が 4 でない拍子（6/8、2/2 など）のクリックの単位は、まだ決まっていない（それまでは四分音符で仮に数える）。
pub fn count_in_beats(score: &Score) -> u32 {
    score
        .measures
        .iter()
        .find(|m| !m.implicit)
        .and_then(|m| u32::try_from(m.time_length.num() / m.time_length.den()).ok())
        .filter(|&b| b > 0)
        .unwrap_or(4)
}

/// 練習のテンポと前カウントから、楽譜の位置 0 の時刻を決める（原点は前カウントの最初のクリック）。
pub fn timing_of(tempo: TempoView, loaded: &Loaded) -> Result<Timing, String> {
    let tempo = Tempo::new(tempo.num, tempo.den).ok_or("テンポが正でない")?;
    Ok(Timing {
        tempo,
        origin_ns: span_to_ns(&tempo, Quarters::new(i64::from(loaded.count_in_beats), 1)),
    })
}

/// 機器を開く（コマンドのスレッドで呼ぶ）。
pub fn open(
    loaded: &Loaded,
    timing: Timing,
    input: Box<dyn MidiInput + Send>,
    record_dir: PathBuf,
) -> Result<OpenSession, StartError> {
    let setup = PracticeSetup {
        expansion: loaded.expansion.clone(),
        score_hash: score_hash(&loaded.bytes),
        timing,
        count_in_beats: loaded.count_in_beats,
        correction_ns: 0,
        record_dir,
        rules: &RULES_V0,
    };
    open_session(setup, input)
}

/// 開いた回を練習のスレッドで進め、通知を `send` に渡す。画面に渡せない時刻（約104日を超える）を含む通知は、
/// 採点と記録には影響しないので、画面への通知だけを落とす。
pub fn run(open: OpenSession, mode: RunMode, stop: &AtomicBool, send: &dyn Fn(LiveEvent)) {
    let notify = |ev| match LiveEvent::from_engine(ev) {
        Ok(live) => send(live),
        Err(e) => eprintln!("画面に渡せない通知: {e}"),
    };
    open.run(mode, &notify, stop);
}

/// この回の記録先（`runs/` の下。名前は始めた時刻のミリ秒）。
pub fn record_dir(root: &Path) -> PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    root.join("runs").join(format!("{ms}"))
}

fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    m.lock().map_err(|e| e.to_string())
}

/// 回を始める印を付ける。進んでいる回があれば断る（`stop_practice` で前の回を止められなくなるため）。
pub fn begin(state: &AppState) -> Result<Arc<AtomicBool>, String> {
    let mut running = lock(&state.running)?;
    if running.is_some() {
        return Err("練習の回がすでに進んでいる".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    *running = Some(Arc::clone(&stop));
    Ok(stop)
}

/// 回が終わった印を付ける。
pub fn end<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(mut running) = app.state::<AppState>().running.lock() {
        *running = None;
    }
}

/// 楽譜を読む。
#[tauri::command]
pub fn load_score(path: String, state: tauri::State<'_, AppState>) -> Result<ScoreView, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("{path} を開けない: {e}"))?;
    let (loaded, view) = load(bytes)?;
    *lock(&state.loaded)? = Some(Arc::new(loaded));
    Ok(view)
}

/// 練習を始める。実機の MIDI の入力は 1a-8 で足すので、今は「MIDI 機器がない」を返す（REQ-103、10-1 の最後の行）。
#[tauri::command]
pub fn start_practice(_tempo: TempoView, _events: Channel<LiveEvent>) -> Result<(), String> {
    Err(StartError::NoMidiDevice.to_string())
}

/// 練習を終える（使う人が閉じた回。記録しない。仕様書 10-1）。進んでいる回がなければ何もしない。
#[tauri::command]
pub fn stop_practice(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(stop) = lock(&state.running)?.as_ref() {
        stop.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// 開発用のビルドだけ（cargo の feature `dev-feed`。REQ-106、AC-23）：MIDI ファイルを流し込んで練習の回を進める。
/// B0 の5章の引数に、練習のテンポ（`tempo`）を足した（流し込みの時刻を楽譜の位置から作るため。計画 2-3 の1）。
#[cfg(feature = "dev-feed")]
#[tauri::command]
pub fn feed_midi_file<R: Runtime>(
    path: String,
    tempo: TempoView,
    events: Channel<LiveEvent>,
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let loaded = lock(&state.loaded)?.clone().ok_or("先に楽譜を読む")?;
    let timing = timing_of(tempo, &loaded)?;
    let midi = std::fs::read(&path).map_err(|e| format!("{path} を開けない: {e}"))?;
    let input = etudition_engine::input::FileInput::new(&midi, &loaded.expansion, &timing)
        .map_err(|e| e.to_string())?;
    let root = state.record_root_or(|| app.path().app_data_dir().map_err(|e| e.to_string()))?;
    let stop = begin(&state)?;
    let open = match open(&loaded, timing, Box::new(input), record_dir(&root)) {
        Ok(o) => o,
        Err(e) => {
            end(&app);
            return Err(e.to_string());
        }
    };
    std::thread::spawn(move || {
        run(open, RunMode::FeedPaced, &stop, &|ev| {
            let _ = events.send(ev);
        });
        end(&app);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::webview::InvokeRequest;

    const T1: &[u8] = include_bytes!("../../../fixtures/1a/t1.musicxml");
    const PICKUP: &[u8] = include_bytes!("../../../fixtures/s1/07-pickup.musicxml");

    fn app() -> (tauri::App<MockRuntime>, tauri::WebviewWindow<MockRuntime>) {
        let app = crate::register(mock_builder())
            .build(mock_context(noop_assets()))
            .expect("作れるはず");
        let w = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("作れるはず");
        (app, w)
    }

    fn invoke(
        w: &tauri::WebviewWindow<MockRuntime>,
        cmd: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, serde_json::Value> {
        get_ipc_response(
            w,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                // 画面のある場所（Windows と Android は http://tauri.localhost、ほかは tauri://localhost）
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|b| {
            b.deserialize::<serde_json::Value>()
                .unwrap_or(serde_json::Value::Null)
        })
    }

    /// AC-23 の AI の部分：登録したコマンドを、本物の呼び出しの経路で確かめる。
    /// 開発用のビルドでない設定では、流し込みのコマンドが「見つからない」。開発用のビルドでは見つかる。
    #[test]
    fn feed_command_is_registered_only_in_dev_build() {
        let (_app, w) = app();
        let err =
            invoke(&w, "feed_midi_file", serde_json::json!({})).expect_err("引数がないので誤り");
        let not_found = err.as_str().is_some_and(|s| s.contains("not found"));
        assert_eq!(not_found, !cfg!(feature = "dev-feed"), "{err}");
        // ほかのコマンドは、どちらのビルドでも見つかる
        let err =
            invoke(&w, "start_practice", serde_json::json!({})).expect_err("引数がないので誤り");
        assert!(
            !err.as_str().is_some_and(|s| s.contains("not found")),
            "{err}"
        );
        assert!(invoke(&w, "stop_practice", serde_json::json!({})).is_ok());
    }

    /// 計画 2-3 の3：画面の呼び出し関数（commands.ts）が、コマンドの名前で呼ぶ。
    #[test]
    fn ui_calls_every_command_by_its_name() {
        let ts = include_str!("../../ui/src/lib/commands.ts");
        for name in [
            "load_score",
            "start_practice",
            "stop_practice",
            "feed_midi_file",
        ] {
            assert!(
                ts.contains(&format!("\"{name}\"")),
                "commands.ts が {name} を呼んでいない"
            );
        }
    }

    #[test]
    fn count_in_is_one_measure() {
        let (t1, _) = load(T1.to_vec()).unwrap();
        assert_eq!(t1.count_in_beats, 4);
        let t = timing_of(TempoView { num: 60, den: 1 }, &t1).unwrap();
        assert_eq!(t.origin_ns, 4_000_000_000);
        // 弱起の小節は数えない（07 は 4/4）
        assert_eq!(load(PICKUP.to_vec()).unwrap().0.count_in_beats, 4);
        // Q-29（人間の回答 2026-11-26）：どの拍子でも1小節。3/4 は3拍
        let three = r#"<?xml version="1.0"?><score-partwise><part-list/><part id="P1"><measure number="1"><attributes><divisions>1</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>3</duration></note></measure></part></score-partwise>"#;
        assert_eq!(load(three.as_bytes().to_vec()).unwrap().0.count_in_beats, 3);
        // 2/4 は2拍
        let two = three
            .replace("<beats>3</beats>", "<beats>2</beats>")
            .replace("<duration>3</duration>", "<duration>2</duration>");
        assert_eq!(load(two.into_bytes()).unwrap().0.count_in_beats, 2);
        // 書かれた音が小節に足りなくても（3/4 に四分音符2つ）、拍子から3拍
        let short = three.replace("<duration>3</duration>", "<duration>2</duration>");
        assert_eq!(load(short.into_bytes()).unwrap().0.count_in_beats, 3);
    }

    /// 進んでいる回があれば、二度目の開始は断る。回が終わった印を付けたら、また始められる。
    #[test]
    fn second_start_is_refused_while_running() {
        let (app, _w) = app();
        let state = app.state::<AppState>();
        let first = begin(&state).expect("始められるはず");
        assert_eq!(
            begin(&state).err().as_deref(),
            Some("練習の回がすでに進んでいる")
        );
        stop_practice(app.state()).unwrap();
        assert!(first.load(Ordering::Relaxed), "止める要求が前の回に届く");
        end(app.handle());
        assert!(begin(&state).is_ok());
    }

    #[test]
    fn unplayable_score_is_refused_with_reasons() {
        let xml = br#"<?xml version="1.0"?><score-partwise><part-list/><part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes><note><rest/><duration>4</duration></note></measure></part></score-partwise>"#;
        let err = load(xml.to_vec()).err().unwrap();
        assert!(err.contains("採点に使えない楽譜"), "{err}");
    }

    /// 1a-6 の結合（AI の部分）：開発用のビルドの入口から流し込みを始め、本物の Channel で音ごとの判定と結果が届く。
    /// 1a-7：通知は時刻どおりに届く（終わりの通知は、終わりの時刻より前には来ない）。速く終わるよう、テンポは 600。
    #[cfg(feature = "dev-feed")]
    #[test]
    fn feed_reaches_the_channel() {
        use std::time::{Duration, Instant};
        let (app, _w) = app();
        let base = std::env::temp_dir().join(format!("etudition-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        app.state::<AppState>()
            .set_record_root(Some(base.clone()))
            .unwrap();
        let score = base.join("t1.musicxml");
        std::fs::write(&score, T1).unwrap();
        let mid = base.join("as-written.mid");
        let (loaded, _) = load(T1.to_vec()).unwrap();
        std::fs::write(&mid, test_midi(&loaded.expansion)).unwrap();
        load_score(score.display().to_string(), app.state()).unwrap();

        let got = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sink = Arc::clone(&got);
        let channel = Channel::<LiveEvent>::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(s) = body {
                sink.lock().unwrap().push(serde_json::from_str(&s).unwrap());
            }
            Ok(())
        });
        let tempo = TempoView { num: 600, den: 1 };
        let began = Instant::now();
        feed_midi_file(
            mid.display().to_string(),
            tempo,
            channel,
            app.handle().clone(),
            app.state(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !got.lock().unwrap().iter().any(|v| v["type"] == "ended") {
            assert!(Instant::now() < deadline, "終わりの通知が届くはず");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            began.elapsed() >= Duration::from_millis(1600),
            "時刻どおりに届くはず"
        );
        let got = got.lock().unwrap().clone();
        assert_eq!(got[0]["type"], "started");
        assert_eq!(got[0]["origin"]["ns"], 400_000_000_i64);
        assert!(got.iter().any(|v| v["type"] == "marks"));
        let ended = got.iter().find(|v| v["type"] == "ended").unwrap();
        assert_eq!(ended["result"]["passed"], true);
        assert_eq!(ended["result"]["end_at"]["ns"], 1_600_000_000_i64);
        assert_eq!(
            ended["result"]["notes"][0]["played"]["on"]["ns"],
            400_000_000_i64
        );
        assert_eq!(ended["record"]["status"], "written");
        // 回が終わったら、印が外れて次の回を始められる
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.state::<AppState>().running.lock().unwrap().is_some() {
            assert!(Instant::now() < deadline, "回の印が外れるはず");
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 楽譜どおりの MIDI（形式 0、960 tick／四分音符）。
    #[cfg(feature = "dev-feed")]
    fn test_midi(e: &Expansion) -> Vec<u8> {
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
