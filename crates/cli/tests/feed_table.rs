//! M-1a の確かめ（AC-06）：採点の仕様 v0 の9章の試験表のうち、`regression.rs`（`errors::CASES`）が
//! MIDI の流し込みで確かめていない 16 行を、MIDI ファイルを `engine` の流し込みで採点して確かめる。
//!
//! 期待値は仕様の表のもの（実装の出力を写したものではない）。時刻はテンポ 60・位置 0 ＝ 4 秒のときの秒。
//! 採点の関数の段の同じ行は `crates/core/tests/scoring_table.rs` が確かめている。
//!
//! 試験だけの工夫：楽譜の頭より前の音（ST-17、ST-27）を MIDI ファイルに置けるように、tick を四分音符
//! [`LEAD`] 個ぶん後ろへずらし、`FileInput` に渡す時刻の原点を同じだけ前にする（採点の時刻は変わらない）。

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use etudition_core::expand::{Expansion, check_playable, expand};
use etudition_core::meter::Meter;
use etudition_core::quarters::Quarters;
use etudition_core::rules::RULES_V0;
use etudition_core::score::read_score;
use etudition_core::scoring::{EndKind, PitchResult, RunResult};
use etudition_core::timing::{Tempo, Timing, position_to_ns, span_to_ns};
use etudition_engine::input::FileInput;
use etudition_engine::record::score_hash;
use etudition_engine::session::{PracticeSetup, RunMode, SessionOutcome, open_session};

const S: i64 = 1_000_000_000;
const MS: i64 = 1_000_000;
/// 四分音符あたりの tick。テンポ 60 で 1 tick ＝ 1 ms（表の時刻はすべて ms で割り切れる）。
const PPQ: i64 = 1000;
/// tick を後ろへずらす四分音符の数（試験だけの工夫。ファイルの頭の説明）。
const LEAD: i64 = 4;

const T1: &[u8] = include_bytes!("../../../fixtures/1a/t1.musicxml");
const T2: &[u8] = include_bytes!("../../../fixtures/1a/t2.musicxml");
const T3: &[u8] = include_bytes!("../../../fixtures/1a/t3.musicxml");
const T4: &[u8] = include_bytes!("../../../fixtures/1a/t4.musicxml");
/// T6 は S1 の 08 と同じ形（仕様の 9-1）。
const T6: &[u8] = include_bytes!("../../../fixtures/s1/08-grace.musicxml");
const T7: &[u8] = include_bytes!("../../../fixtures/1a/t7.musicxml");

fn expansion(xml: &[u8]) -> Expansion {
    let e = expand(&read_score(xml).expect("読めるはず").score);
    check_playable(&e).expect("使えるはず");
    e
}

fn timing() -> Timing {
    Timing {
        tempo: Tempo::new(60, 1).unwrap(),
        origin_ns: 4 * S,
    }
}

/// 演奏の1音（ノート番号、ノートオン、ノートオフ。ノートオフが `None` なら送らない）。
#[derive(Clone, Copy)]
struct P {
    note: u8,
    on: i64,
    off: Option<i64>,
}

/// 楽譜どおりの演奏。
fn as_written(e: &Expansion) -> Vec<P> {
    let t = timing();
    e.sounding
        .iter()
        .map(|x| {
            let on = position_to_ns(&t, x.start);
            P {
                note: x.pitch as u8,
                on,
                off: Some(on + span_to_ns(&t.tempo, x.duration)),
            }
        })
        .collect()
}

fn shift(p: &mut P, by: i64) {
    p.on += by;
    p.off = p.off.map(|o| o + by);
}

/// 時刻を tick にする。割り切れない時刻は試験の誤りとして止める。
fn tick(e: &Expansion, ns: i64) -> i64 {
    let pickup = e.measures.first().map_or(Quarters::ZERO, |m| {
        Quarters::ZERO - m.start.min(Quarters::ZERO)
    });
    let lead = pickup + Quarters::new(LEAD, 1);
    assert_eq!((lead.num() * PPQ) % lead.den(), 0);
    let d = ns - timing().origin_ns;
    assert_eq!(d % MS, 0, "{ns} ns は 1 ms で割り切れるはず");
    let t = d / MS + lead.num() * PPQ / lead.den();
    assert!(t >= 0, "{ns} ns は MIDI ファイルの頭より後のはず");
    t
}

/// 演奏を MIDI（形式 0）にする。同じ tick ではノートオフが先。`zero_velocity_off` ならノートオフを
/// ベロシティ 0 のノートオンで送る。
fn midi(e: &Expansion, perf: &[P], zero_velocity_off: bool) -> Vec<u8> {
    use midly::num::{u4, u7, u15, u28};
    use midly::{
        Format, Header, MetaMessage, MidiMessage, Smf, Timing as MidiTiming, TrackEvent,
        TrackEventKind,
    };
    // (tick, オンか, 受け取る順, ノート番号)
    let mut evs: Vec<(i64, bool, usize, u8)> = Vec::new();
    for (i, p) in perf.iter().enumerate() {
        evs.push((tick(e, p.on), true, i, p.note));
        if let Some(off) = p.off {
            evs.push((tick(e, off), false, i, p.note));
        }
    }
    evs.sort_unstable();
    let mut track = Vec::new();
    let mut prev = 0;
    for (t, on, _, key) in evs {
        let message = match (on, zero_velocity_off) {
            (true, _) => MidiMessage::NoteOn {
                key: u7::new(key),
                vel: u7::new(100),
            },
            (false, true) => MidiMessage::NoteOn {
                key: u7::new(key),
                vel: u7::new(0),
            },
            (false, false) => MidiMessage::NoteOff {
                key: u7::new(key),
                vel: u7::new(0),
            },
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
        header: Header::new(
            Format::SingleTrack,
            MidiTiming::Metrical(u15::new(PPQ as u16)),
        ),
        tracks: vec![track],
    };
    let mut out = Vec::new();
    smf.write_std(&mut out).unwrap();
    out
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "etudition-feed-table-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// MIDI ファイルを流し込みで採点する。
fn feed(name: &str, xml: &[u8], perf: &[P], zero_velocity_off: bool) -> RunResult {
    let e = expansion(xml);
    let bytes = midi(&e, perf, zero_velocity_off);
    // 試験だけの工夫：tick を LEAD だけ後ろへずらしたので、原点を同じだけ前にする
    let input_timing = Timing {
        origin_ns: timing().origin_ns - LEAD * S,
        ..timing()
    };
    let input = FileInput::new(&bytes, &e, &input_timing).expect("MIDI を読めるはず");
    let dir = tmp(name);
    let setup = PracticeSetup {
        expansion: e,
        score_hash: score_hash(xml),
        timing: timing(),
        meter: Meter {
            beats: 4,
            beat_type: 4,
        },
        correction_ns: 0,
        record_dir: dir.clone(),
        rules: RULES_V0,
    };
    let open = open_session(setup, Box::new(input), None).expect("開けるはず");
    let outcome = open.run(RunMode::Feed, &|_| {}, &AtomicBool::new(false));
    let _ = std::fs::remove_dir_all(&dir);
    let SessionOutcome::Scored(result) = outcome else {
        panic!("{name}：採点されるはず: {outcome:?}");
    };
    result
}

/// （音程、タイミング、長さの誤りの数、組の数）。
fn counts(r: &RunResult) -> (u32, u32, u32, usize) {
    let i = r.items.expect("点数があるはず");
    let pairs = r.notes.iter().filter(|x| x.played.is_some()).count();
    (i.pitch.errors, i.timing.errors, i.duration.errors, pairs)
}

fn end(r: &RunResult) -> (EndKind, i64) {
    (r.end.kind, r.end.at_ns)
}

/// お手本の1音（音高、位置、長さ、装飾音か）。
type Ref = (i32, Quarters, Quarters, bool);

/// 試験の楽譜が、仕様の 9-1 のお手本（音高、位置、長さ、装飾音か。並びは SR-01）のとおりか。
#[test]
fn fixtures_match_the_spec() {
    let q = Quarters::new;
    let cases: [(&str, &[u8], Vec<Ref>); 5] = [
        (
            "T2",
            T2,
            vec![
                (60, q(0, 1), q(1, 1), false),
                (62, q(1, 1), q(1, 1), false),
                (64, q(2, 1), q(1, 1), false),
                (65, q(3, 1), q(1, 1), false),
            ],
        ),
        (
            "T3",
            T3,
            vec![(60, q(0, 1), q(1, 1), false), (62, q(8, 1), q(1, 1), false)],
        ),
        (
            "T4",
            T4,
            vec![
                (55, q(-1, 1), q(1, 1), false),
                (60, q(0, 1), q(4, 1), false),
            ],
        ),
        (
            "T6",
            T6,
            vec![
                (72, q(0, 1), q(1, 1), false),
                (74, q(0, 1), q(0, 1), true),
                (76, q(1, 1), q(1, 1), false),
                (79, q(2, 1), q(2, 1), false),
            ],
        ),
        (
            "T7",
            T7,
            vec![
                (60, q(0, 1), q(1, 1), false),
                (62, q(1, 1), q(1, 1), false),
                (64, q(2, 1), q(1, 2), false),
                (65, q(5, 2), q(1, 2), false),
            ],
        ),
    ];
    for (name, xml, want) in cases {
        let got: Vec<_> = expansion(xml)
            .sounding
            .iter()
            .map(|s| (s.pitch, s.start, s.duration, s.is_grace))
            .collect();
        assert_eq!(got, want, "{name}");
    }
}

/// ST-13：和音を高い音から 20 ms ずつずらす（G4 ＋0、E4 ＋20 ms、C4 ＋40 ms）。
#[test]
fn st13_rolled_chord() {
    let mut perf = as_written(&expansion(T1));
    perf[5].on += 20 * MS;
    perf[4].on += 40 * MS;
    let r = feed("st13", T1, &perf, false);
    assert_eq!(counts(&r), (0, 0, 0, 10));
    assert_eq!(end(&r), (EndKind::Normal, 16 * S));
}

/// ST-15：ノートオフをすべてベロシティ 0 のノートオンで送る。
#[test]
fn st15_velocity_zero_offs() {
    let perf = as_written(&expansion(T1));
    let r = feed("st15", T1, &perf, true);
    assert_eq!(counts(&r), (0, 0, 0, 10));
    assert_eq!(end(&r), (EndKind::Normal, 16 * S));
}

/// ST-17：C4 を位置 −1/2〜−1/4（範囲の始まりちょうど 3.5 秒）に足す。足した音は余分。
#[test]
fn st17_extra_note_at_range_start() {
    let mut perf = as_written(&expansion(T1));
    perf.push(P {
        note: 60,
        on: 3 * S + S / 2,
        off: Some(3 * S + 3 * S / 4),
    });
    let r = feed("st17", T1, &perf, false);
    assert_eq!(counts(&r), (1, 0, 0, 10));
    assert_eq!(r.extras.len(), 1);
    assert_eq!(r.extras[0].on_ns, 3 * S + S / 2);
}

/// ST-21：16.5 秒（r10 を離した後）に D4 を足す。正常（16 秒）で、足した音は捨てる。
#[test]
fn st21_input_after_the_end_is_dropped() {
    let mut perf = as_written(&expansion(T1));
    perf.push(P {
        note: 62,
        on: 16 * S + S / 2,
        off: Some(16 * S + 4 * S / 5),
    });
    let r = feed("st21", T1, &perf, false);
    assert_eq!(counts(&r), (0, 0, 0, 10));
    assert_eq!(end(&r), (EndKind::Normal, 16 * S));
    assert!(r.extras.is_empty());
}

/// ST-22：r2 を D#4 にし、r4 の後は弾かない。止まった（13 秒）。誤りの場所は r2。q と合否なし。
#[test]
fn st22_stopped_run_reports_places() {
    let mut perf = as_written(&expansion(T1));
    perf[1].note = 63;
    perf.truncate(4);
    let r = feed("st22", T1, &perf, false);
    assert_eq!(end(&r), (EndKind::Stopped, 13 * S));
    assert_eq!((r.items, r.passed), (None, None));
    assert_eq!(r.stopped_at, Some(4));
    let pitches: Vec<PitchResult> = r.notes.iter().map(|x| x.pitch).collect();
    assert_eq!(
        pitches,
        [
            PitchResult::Correct,
            PitchResult::WrongPitch,
            PitchResult::Correct,
            PitchResult::Correct
        ]
    );
    assert!(r.extras.is_empty());
}

/// ST-23：T2 で r4 を弾かない。止まった（12 秒。上限の 11 秒は使わない）。
#[test]
fn st23_stops_without_the_last_note() {
    let mut perf = as_written(&expansion(T2));
    perf.truncate(3);
    let r = feed("st23", T2, &perf, false);
    assert_eq!(end(&r), (EndKind::Stopped, 12 * S));
    assert_eq!((r.items, r.passed), (None, None));
}

/// ST-24：T2 で r4 を G4 にする。正常（8 秒）。
#[test]
fn st24_last_note_wrong() {
    let mut perf = as_written(&expansion(T2));
    perf[3].note = 67;
    let r = feed("st24", T2, &perf, false);
    assert_eq!(counts(&r), (1, 0, 0, 4));
    assert_eq!(end(&r), (EndKind::Normal, 8 * S));
}

/// ST-25：T3 を楽譜どおりに。止まらない（D4 を離した 13 秒に正常）。
#[test]
fn st25_long_rest() {
    let perf = as_written(&expansion(T3));
    let r = feed("st25", T3, &perf, false);
    assert_eq!(counts(&r), (0, 0, 0, 2));
    assert_eq!(end(&r), (EndKind::Normal, 13 * S));
}

/// ST-26：T4 を楽譜どおりに。弱起の音（3 秒）を採点する。
#[test]
fn st26_pickup() {
    let perf = as_written(&expansion(T4));
    assert_eq!(perf[0].on, 3 * S);
    let r = feed("st26", T4, &perf, false);
    assert_eq!(counts(&r), (0, 0, 0, 2));
    assert_eq!(end(&r), (EndKind::Normal, 8 * S));
}

/// ST-27：T4 で C4 を位置 −2〜−3/2（2 秒。範囲の始まりより前）に足す。余分にも数えない。
#[test]
fn st27_before_the_range() {
    let mut perf = as_written(&expansion(T4));
    perf.push(P {
        note: 60,
        on: 2 * S,
        off: Some(2 * S + S / 2),
    });
    let r = feed("st27", T4, &perf, false);
    assert_eq!(counts(&r), (0, 0, 0, 2));
    assert!(r.extras.is_empty());
}

/// T6 の演奏（装飾音は主音 4 秒からの ms で、ノートオンとノートオフ。`None` なら弾かない）。
fn t6_perf(grace_ms: Option<(i64, i64)>) -> Vec<P> {
    let mut perf = vec![
        P {
            note: 72,
            on: 4 * S,
            off: Some(5 * S),
        },
        P {
            note: 76,
            on: 5 * S,
            off: Some(6 * S),
        },
        P {
            note: 79,
            on: 6 * S,
            off: Some(8 * S),
        },
    ];
    if let Some((on, off)) = grace_ms {
        perf.insert(
            0,
            P {
                note: 74,
                on: 4 * S + on * MS,
                off: Some(4 * S + off * MS),
            },
        );
    }
    perf
}

/// ST-30：装飾音を主音の 60 ms 前にノートオン、10 ms 前にノートオフ。長さの分母 3。
#[test]
fn st30_grace_played_slightly_early() {
    let r = feed("st30", T6, &t6_perf(Some((-60, -10))), false);
    assert_eq!(counts(&r), (0, 0, 0, 4));
    assert_eq!(r.items.unwrap().duration.denominator, 3);
}

/// ST-31：装飾音を弾かない。
#[test]
fn st31_grace_not_played() {
    let r = feed("st31", T6, &t6_perf(None), false);
    assert_eq!(counts(&r), (1, 0, 0, 3));
    assert_eq!(r.notes[1].pitch, PitchResult::Missed);
}

/// ST-32：装飾音を主音の 200 ms 前にノートオン、150 ms 前にノートオフ（6章の限界：抜け＋余分）。
#[test]
fn st32_grace_played_too_early() {
    let r = feed("st32", T6, &t6_perf(Some((-200, -150))), false);
    assert_eq!(counts(&r), (2, 0, 0, 3));
}

/// ST-33：T2 で r4 を 3/4 拍遅らせる（7.75〜8.75 秒）。正常（8.75 秒）。r4 は抜け、F4 は余分。
#[test]
fn st33_last_note_outside_its_window() {
    let mut perf = as_written(&expansion(T2));
    shift(&mut perf[3], 3 * S / 4);
    let r = feed("st33", T2, &perf, false);
    assert_eq!(counts(&r), (2, 0, 0, 3));
    assert_eq!(end(&r), (EndKind::Normal, 8 * S + 3 * S / 4));
}

/// ST-36：T7 で r1〜r3 を楽譜どおりに弾き、r4 は弾かない。止まった（11.5 秒）。r3 は誤りに出ない。
#[test]
fn st36_stop_position_keeps_the_previous_note() {
    let mut perf = as_written(&expansion(T7));
    perf.truncate(3);
    let r = feed("st36", T7, &perf, false);
    assert_eq!(end(&r), (EndKind::Stopped, 11 * S + S / 2));
    assert_eq!((r.items, r.passed), (None, None));
    assert_eq!(r.stopped_at, Some(3));
    assert_eq!(r.notes.len(), 3);
    assert!(r.notes.iter().all(|x| x.pitch == PitchResult::Correct));
    assert!(r.extras.is_empty());
}

/// ST-37：T2 で r1〜r3 を楽譜どおりに弾き、F4 を 11.5 秒に初めて押す。上限（11.5 秒）。r4 は抜け、F4 は余分。
#[test]
fn st37_first_press_after_the_cap() {
    let mut perf = as_written(&expansion(T2));
    perf[3] = P {
        note: 65,
        on: 11 * S + S / 2,
        off: None,
    };
    let r = feed("st37", T2, &perf, false);
    assert_eq!(counts(&r), (2, 0, 0, 3));
    assert_eq!(end(&r), (EndKind::Cap, 11 * S + S / 2));
}
