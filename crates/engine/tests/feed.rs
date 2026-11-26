//! 1a-4：流し込みから採点と記録まで（AC-10、AC-21 の書いて読み戻す部分、AC-42 の (1)(3)）。
//! 1a-9 の engine の部分：接続が切れた回を無効にする（REQ-103、AC-43 の切断の部分）。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use etudition_core::expand::{Expansion, check_playable, expand};
use etudition_core::rules::RULES_V0;
use etudition_core::score::read_score;
use etudition_core::scoring::{EndKind, RunResult};
use etudition_core::timing::{Tempo, Timing};
use etudition_engine::input::FileInput;
use etudition_engine::record::{RecordStatus, score_hash};
use etudition_engine::session::{
    EngineEvent, InvalidReason, PracticeSetup, RunMode, SessionOutcome, open_session,
};

const S: i64 = 1_000_000_000;
const PPQ: i64 = 960;

const PICKUP: &[u8] = include_bytes!("../../../fixtures/s1/07-pickup.musicxml");
const TIE: &[u8] = include_bytes!("../../../fixtures/s1/01-tie.musicxml");

fn expansion(score: &[u8]) -> Expansion {
    let e = expand(&read_score(score).expect("読めるはず").score);
    check_playable(&e).expect("使えるはず");
    e
}

/// お手本から、楽譜どおりの MIDI（弱起の音から始まる形）を作る。`drop_last` なら最後の音を弾かない。
fn as_written_midi(e: &Expansion, drop_last: bool) -> Vec<u8> {
    use midly::num::{u4, u7, u15, u28};
    use midly::{
        Format, Header, MetaMessage, MidiMessage, Smf, Timing as T, TrackEvent, TrackEventKind,
    };
    let pickup = e
        .measures
        .first()
        .map_or(0, |m| (-m.start.num() * PPQ / m.start.den()).max(0));
    let tick = |q: etudition_core::quarters::Quarters| q.num() * PPQ / q.den() + pickup;
    let n = e.sounding.len() - usize::from(drop_last);
    // (tick, ノートオフが先, ノート番号, オンか)
    let mut evs: Vec<(i64, u8, u8, bool)> = Vec::new();
    for s in &e.sounding[..n] {
        evs.push((tick(s.start), 1, s.pitch as u8, true));
        evs.push((tick(s.start + s.duration), 0, s.pitch as u8, false));
    }
    evs.sort();
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
        header: Header::new(Format::SingleTrack, T::Metrical(u15::new(PPQ as u16))),
        tracks: vec![track],
    };
    let mut out = Vec::new();
    smf.write_std(&mut out).unwrap();
    out
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("etudition-feed-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn timing(tempo: u32, origin_ns: i64) -> Timing {
    Timing {
        tempo: Tempo::new(tempo, 1).unwrap(),
        origin_ns,
    }
}

struct Run {
    outcome: SessionOutcome,
    events: Vec<EngineEvent>,
}

/// 流し込みの設定（既定：補正 0、切断なし、流し込み、閉じない）。
#[derive(Clone, Copy)]
struct Opts {
    correction_ns: i64,
    disconnect_at: Option<i64>,
    mode: RunMode,
    stop_now: bool,
}

const FEED: Opts = Opts {
    correction_ns: 0,
    disconnect_at: None,
    mode: RunMode::Feed,
    stop_now: false,
};

fn feed(score: &[u8], midi: &[u8], t: Timing, dir: &Path, o: Opts) -> Run {
    let e = expansion(score);
    let mut input = FileInput::new(midi, &e, &t).expect("MIDI を読めるはず");
    if let Some(at) = o.disconnect_at {
        input = input.disconnect_at(at);
    }
    let setup = PracticeSetup {
        expansion: e,
        score_hash: score_hash(score),
        timing: t,
        count_in_beats: 4,
        correction_ns: o.correction_ns,
        record_dir: dir.to_path_buf(),
        rules: &RULES_V0,
    };
    let open = open_session(setup, Box::new(input)).expect("開けるはず");
    let seen = Mutex::new(Vec::new());
    let stop = AtomicBool::new(o.stop_now);
    let outcome = open.run(o.mode, &|ev| seen.lock().unwrap().push(ev), &stop);
    Run {
        outcome,
        events: seen.into_inner().unwrap(),
    }
}

fn scored(r: &Run) -> &RunResult {
    match &r.outcome {
        SessionOutcome::Scored(res) => res,
        o => panic!("採点されるはず: {o:?}"),
    }
}

/// AC-21・AC-10：楽譜どおりの MIDI を流すと誤り 0 で正常に終わり、result.json を読み戻すと値が流した値と一致する。
/// 弱起の音（前カウントの間）も採点する（AC-14）。
#[test]
fn feed_as_written_scores_and_records() {
    let dir = tmp("as-written");
    let t = timing(60, 4 * S);
    let e = expansion(PICKUP);
    let r = feed(
        PICKUP,
        &as_written_midi(&e, false),
        t,
        &dir.join("run"),
        FEED,
    );
    let res = scored(&r);
    assert_eq!(res.end.kind, EndKind::Normal);
    let items = res.items.unwrap();
    assert_eq!(
        (
            items.pitch.errors,
            items.timing.errors,
            items.duration.errors
        ),
        (0, 0, 0)
    );
    assert_eq!(res.passed, Some(true));
    assert!(matches!(
        r.events.first(),
        Some(EngineEvent::Started { .. })
    ));
    assert!(
        r.events
            .iter()
            .any(|e| matches!(e, EngineEvent::NoteReceived { .. }))
    );
    assert!(r.events.iter().any(|e| matches!(e, EngineEvent::Marks(_))));
    let Some(EngineEvent::Ended {
        record: RecordStatus::Written(files),
        ..
    }) = r.events.last()
    else {
        panic!("記録したはず: {:?}", r.events.last());
    };
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files.json.as_ref().unwrap()).unwrap()).unwrap();
    assert_eq!(json["rules_version"], "v0");
    assert_eq!(json["score_sha256"], score_hash(PICKUP));
    assert_eq!(json["tempo"]["num"], 60);
    assert_eq!(json["tempo"]["den"], 1);
    assert_eq!(json["count_in_end_ns"], 4 * S);
    assert_eq!(json["correction_ns"], 0);
    // 弱起の G4 は 3 秒（前カウントの4拍目）
    assert_eq!(json["events"][0]["raw_ns"], 3 * S);
    assert_eq!(json["events"][0]["corrected_ns"], 3 * S);
    assert_eq!(json["events"].as_array().unwrap().len(), 6);
    assert_eq!(json["result"]["items"]["pitch"]["q"], 1_000_000);
    assert_eq!(json["result"]["passed"], true);
    let mid = std::fs::read(files.midi.as_ref().unwrap()).unwrap();
    let smf = midly::Smf::parse(&mid).unwrap();
    let notes = smf.tracks[0]
        .iter()
        .filter(|e| matches!(e.kind, midly::TrackEventKind::Midi { .. }))
        .count();
    assert_eq!(notes, 6);
    let _ = std::fs::remove_dir_all(&dir);
}

/// SR-02、AC-10：補正の値を引いた時刻で採点し、生の時刻と補正後の時刻を両方記録する。
#[test]
fn correction_is_recorded_and_applied() {
    let dir = tmp("correction");
    let t = timing(60, 4 * S);
    let e = expansion(TIE);
    let r = feed(
        TIE,
        &as_written_midi(&e, false),
        t,
        &dir.join("run"),
        Opts {
            correction_ns: 5_000_000,
            ..FEED
        },
    );
    let res = scored(&r);
    assert_eq!(res.items.unwrap().timing.errors, 0);
    assert_eq!(res.notes[0].delta_ns, Some(-5_000_000));
    let Some(EngineEvent::Ended {
        record: RecordStatus::Written(files),
        ..
    }) = r.events.last()
    else {
        panic!("記録したはず");
    };
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files.json.as_ref().unwrap()).unwrap()).unwrap();
    assert_eq!(json["correction_ns"], 5_000_000);
    assert_eq!(json["events"][0]["raw_ns"], 4 * S);
    assert_eq!(json["events"][0]["corrected_ns"], 4 * S - 5_000_000);
    let _ = std::fs::remove_dir_all(&dir);
}

/// REQ-103（1a-9 の engine の部分）：接続が切れた回は無効にし、採点せず、result.json に理由を書く。
/// performance.mid は未確定（Q-51）なので書かない。
#[test]
fn disconnect_makes_the_run_invalid() {
    let dir = tmp("disconnect");
    let t = timing(60, 4 * S);
    let e = expansion(TIE);
    let r = feed(
        TIE,
        &as_written_midi(&e, false),
        t,
        &dir.join("run"),
        Opts {
            disconnect_at: Some(5 * S),
            ..FEED
        },
    );
    assert_eq!(
        r.outcome,
        SessionOutcome::Invalid(InvalidReason::Disconnected)
    );
    let Some(EngineEvent::Invalid {
        record: RecordStatus::Written(files),
        ..
    }) = r.events.last()
    else {
        panic!("無効の回を記録したはず: {:?}", r.events.last());
    };
    assert!(files.midi.is_none());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files.json.as_ref().unwrap()).unwrap()).unwrap();
    assert_eq!(json["invalid_reason"], "MIDI の接続が切れた");
    assert!(json["result"].is_null());
    let _ = std::fs::remove_dir_all(&dir);
}

/// 止まった回の記録は未確定（Q-51）なので書かない。
#[test]
fn stopped_run_is_not_recorded() {
    let dir = tmp("stopped");
    let t = timing(60, 4 * S);
    let e = expansion(TIE);
    let r = feed(TIE, &as_written_midi(&e, true), t, &dir.join("run"), FEED);
    assert_eq!(scored(&r).end.kind, EndKind::Stopped);
    assert!(matches!(
        r.events.last(),
        Some(EngineEvent::Ended {
            record: RecordStatus::NotWritten,
            ..
        })
    ));
    assert!(!dir.join("run").exists());
}

/// AC-42 の (1)：書き込めない記録先なら、採点の結果は返り、「記録できなかった」になり、書きかけが残らない。
#[test]
fn unwritable_record_dir_fails_cleanly() {
    let base = tmp("unwritable");
    std::fs::create_dir_all(&base).unwrap();
    let file = base.join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();
    let t = timing(60, 4 * S);
    let e = expansion(TIE);
    let r = feed(TIE, &as_written_midi(&e, false), t, &file.join("run"), FEED);
    assert_eq!(scored(&r).passed, Some(true));
    assert!(matches!(
        r.events.last(),
        Some(EngineEvent::Ended {
            record: RecordStatus::Failed(_),
            ..
        })
    ));
    let left: Vec<_> = std::fs::read_dir(&base)
        .unwrap()
        .map(|d| d.unwrap().file_name())
        .collect();
    assert_eq!(left, ["not-a-dir"]);
    let _ = std::fs::remove_dir_all(&base);
}

/// AC-42 の (3)：途中で閉じた回は、何も記録しない。
#[test]
fn cancelled_run_writes_nothing() {
    let dir = tmp("cancel");
    let t = timing(60, 4 * S);
    let e = expansion(TIE);
    let r = feed(
        TIE,
        &as_written_midi(&e, false),
        t,
        &dir.join("run"),
        Opts {
            stop_now: true,
            ..FEED
        },
    );
    assert_eq!(r.outcome, SessionOutcome::Cancelled);
    assert!(!dir.exists());
}

/// 時計で進む回（Live）も、同じ採点と記録の経路を通る。テンポ 600 で約 1 秒。
#[test]
fn live_mode_reaches_the_same_result() {
    let dir = tmp("live");
    let t = timing(600, 0);
    let e = expansion(TIE);
    let r = feed(
        TIE,
        &as_written_midi(&e, false),
        t,
        &dir.join("run"),
        Opts {
            mode: RunMode::Live,
            ..FEED
        },
    );
    let res = scored(&r);
    assert_eq!(res.end.kind, EndKind::Normal);
    assert_eq!(res.end.at_ns, 8 * S / 10);
    let items = res.items.unwrap();
    assert_eq!(
        (
            items.pitch.errors,
            items.timing.errors,
            items.duration.errors
        ),
        (0, 0, 0)
    );
    assert!(r.events.iter().any(|e| matches!(e, EngineEvent::Marks(_))));
    let _ = std::fs::remove_dir_all(&dir);
}
