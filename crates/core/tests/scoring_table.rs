//! 採点の仕様 v0 の9章の試験表（ST-xx）を、採点の関数で確かめる（1a-2）。
//!
//! 期待値は仕様の表のもの（実装の出力を写したものではない）。時刻はテンポ 60・位置 0 ＝ 4 秒のときの秒。
//! 回の終わりは、表の「終わり方」の欄の時刻を渡す（終わりを決める `decide_end` は 1a-9）。
//! 止まった回の行（ST-22、ST-23、ST-36）は 1a-9 で確かめる。

use etudition_core::expand::SoundingNote;
use etudition_core::perf::{PerfEvent, PerfKind};
use etudition_core::quarters::Quarters;
use etudition_core::rules::RULES_V0;
use etudition_core::scoring::{
    EndKind, PitchResult, RunEnd, RunInput, RunResult, TimingResult, marks_closed_between,
    score_run,
};
use etudition_core::timing::{Tempo, Timing, position_to_ns, span_to_ns};

const S: i64 = 1_000_000_000;
const MS: i64 = 1_000_000;

/// お手本の1音（位置と長さは四分音符の数の分数）。
fn r(pitch: i32, start: (i64, i64), dur: (i64, i64), grace: bool) -> SoundingNote {
    SoundingNote {
        pitch,
        start: Quarters::new(start.0, start.1),
        duration: Quarters::new(dur.0, dur.1),
        is_grace: grace,
        written: Vec::new(),
    }
}

fn n(pitch: i32, start: i64, dur: i64) -> SoundingNote {
    r(pitch, (start, 1), (dur, 1), false)
}

/// T1：お手本10音（並びは SR-01。r5〜r7 の和音は C4、E4、G4 の順）。
fn t1() -> Vec<SoundingNote> {
    vec![
        n(60, 0, 1),
        n(62, 1, 1),
        n(64, 2, 1),
        n(65, 3, 1),
        n(60, 4, 2),
        n(64, 4, 2),
        n(67, 4, 2),
        n(67, 6, 1),
        n(67, 7, 1),
        n(72, 8, 4),
    ]
}

fn t2() -> Vec<SoundingNote> {
    vec![n(60, 0, 1), n(62, 1, 1), n(64, 2, 1), n(65, 3, 1)]
}

fn t3() -> Vec<SoundingNote> {
    vec![n(60, 0, 1), n(62, 8, 1)]
}

fn t4() -> Vec<SoundingNote> {
    vec![n(55, -1, 1), n(60, 0, 4)]
}

/// T6：装飾音 D5 と主音 C5 は同じ位置（SR-19）。並びは音高の順で C5、D5。
fn t6() -> Vec<SoundingNote> {
    vec![
        n(72, 0, 1),
        r(74, (0, 1), (0, 1), true),
        n(76, 1, 1),
        n(79, 2, 2),
    ]
}

fn timing(tempo: u32) -> Timing {
    Timing {
        tempo: Tempo::new(tempo, 1).unwrap(),
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
fn as_written(refs: &[SoundingNote], t: &Timing) -> Vec<P> {
    refs.iter()
        .map(|x| {
            let on = position_to_ns(t, x.start);
            P {
                note: x.pitch as u8,
                on,
                off: Some(on + span_to_ns(&t.tempo, x.duration)),
            }
        })
        .collect()
}

/// イベントの列にする（時刻の順。同じ時刻はノートオフが先）。`zero_velocity_off` ならノートオフをベロシティ 0 のノートオンで送る。
fn events(perf: &[P], zero_velocity_off: bool) -> Vec<PerfEvent> {
    let mut e = Vec::new();
    for p in perf {
        e.push(PerfEvent {
            t_ns: p.on,
            kind: PerfKind::On,
            note: p.note,
            velocity: 100,
        });
        if let Some(off) = p.off {
            e.push(PerfEvent {
                t_ns: off,
                kind: if zero_velocity_off {
                    PerfKind::On
                } else {
                    PerfKind::Off
                },
                note: p.note,
                velocity: 0,
            });
        }
    }
    e.sort_by_key(|x| (x.t_ns, x.velocity != 0 && x.kind == PerfKind::On));
    e
}

fn run(refs: &[SoundingNote], t: Timing, perf: &[P], end: RunEnd) -> RunResult {
    let ev = events(perf, false);
    score_run(
        &RunInput {
            reference: refs,
            timing: t,
            events: &ev,
            rules: &RULES_V0,
        },
        end,
    )
}

fn normal(at: i64) -> RunEnd {
    RunEnd {
        kind: EndKind::Normal,
        at_ns: at,
    }
}

/// （音程、タイミング、長さの誤りの数、組の数）。
fn counts(res: &RunResult) -> (u32, u32, u32, usize) {
    let i = res.items.expect("点数があるはず");
    let pairs = res.notes.iter().filter(|x| x.played.is_some()).count();
    (i.pitch.errors, i.timing.errors, i.duration.errors, pairs)
}

fn shift(p: &mut P, by: i64) {
    p.on += by;
    p.off = p.off.map(|o| o + by);
}

#[test]
fn st01_as_written_twice() {
    let t = timing(60);
    let perf = as_written(&t1(), &t);
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (0, 0, 0, 10));
    let i = a.items.unwrap();
    assert_eq!(
        (i.pitch.q, i.timing.q, i.duration.q),
        (1_000_000, 1_000_000, 1_000_000)
    );
    assert_eq!(a.passed, Some(true));
    assert_eq!(a.rules_version, "v0");
    // AC-07、SR-13：同じ入力なら同じ出力
    assert_eq!(run(&t1(), t, &perf, normal(16 * S)), a);
}

#[test]
fn st02_wrong_pitch() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf[1].note = 63;
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (1, 0, 0, 10));
    assert_eq!(a.items.unwrap().pitch.q, 900_000);
    assert_eq!(a.passed, Some(true));
    assert_eq!(a.notes[1].pitch, PitchResult::WrongPitch);
}

#[test]
fn st03_octave_is_a_pitch_error() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf[0].note = 72;
    assert_eq!(counts(&run(&t1(), t, &perf, normal(16 * S))), (1, 0, 0, 10));
}

#[test]
fn st04_missing_note() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf.remove(1);
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (1, 0, 0, 9));
    assert_eq!(a.notes[1].pitch, PitchResult::Missed);
}

#[test]
fn st05_extra_note_in_the_window() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf.push(P {
        note: 69,
        on: 4 * S + S / 4,
        off: Some(4 * S + S / 2),
    });
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (1, 0, 0, 10));
    assert_eq!(a.extras.len(), 1);
    assert_eq!(a.extras[0].note, 69);
}

#[test]
fn st06_st07_timing() {
    let t = timing(60);
    let mut late = as_written(&t1(), &t);
    shift(&mut late[1], S / 4);
    let a = run(&t1(), t, &late, normal(16 * S));
    assert_eq!(counts(&a), (0, 1, 0, 10));
    assert_eq!(a.notes[1].timing, TimingResult::Late);
    let mut edge = as_written(&t1(), &t);
    shift(&mut edge[1], S / 8);
    assert_eq!(counts(&run(&t1(), t, &edge, normal(16 * S))), (0, 0, 0, 10));
}

#[test]
fn st08_outside_the_window() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    shift(&mut perf[1], 3 * S / 4);
    assert_eq!(counts(&run(&t1(), t, &perf, normal(16 * S))), (2, 0, 0, 9));
}

#[test]
fn st09_duration_boundaries() {
    let t = timing(60);
    for (len_ms, errors) in [(400, 1), (500, 0), (1500, 0), (1600, 1)] {
        let mut perf = as_written(&t1(), &t);
        perf[1].off = Some(perf[1].on + len_ms * MS);
        assert_eq!(
            counts(&run(&t1(), t, &perf, normal(16 * S))),
            (0, 0, errors, 10),
            "長さ {len_ms} ms"
        );
    }
}

#[test]
fn st12_missing_chord_note() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf.remove(5);
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (1, 0, 0, 9));
    assert_eq!(a.notes[5].pitch, PitchResult::Missed);
}

#[test]
fn st13_rolled_chord() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf[5].on += 20 * MS;
    perf[4].on += 40 * MS;
    // 高い音から：G4 ＋0、E4 ＋20 ms、C4 ＋40 ms。受け取る順も高い音から
    perf.swap(4, 6);
    assert_eq!(counts(&run(&t1(), t, &perf, normal(16 * S))), (0, 0, 0, 10));
}

#[test]
fn st14_missing_repeated_note() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf.remove(8);
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (1, 0, 0, 9));
    assert_eq!(a.notes[8].pitch, PitchResult::Missed);
}

#[test]
fn st15_velocity_zero_offs() {
    let t = timing(60);
    let perf = as_written(&t1(), &t);
    let ev = events(&perf, true);
    let a = score_run(
        &RunInput {
            reference: &t1(),
            timing: t,
            events: &ev,
            rules: &RULES_V0,
        },
        normal(16 * S),
    );
    assert_eq!(counts(&a), (0, 0, 0, 10));
}

#[test]
fn st17_extra_note_at_range_start() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf.push(P {
        note: 60,
        on: 3 * S + S / 2,
        off: Some(3 * S + 3 * S / 4),
    });
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (1, 0, 0, 10));
    assert_eq!(a.extras[0].on_ns, 3 * S + S / 2);
}

/// ST-18、AC-09：タイミングだけを誤ると、タイミングの q は 0、音程と長さは 100 点で合格。
#[test]
fn st18_all_late() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf.iter_mut().for_each(|p| shift(p, S / 4));
    let a = run(&t1(), t, &perf, normal(16 * S + S / 4));
    assert_eq!(counts(&a), (0, 10, 0, 10));
    let i = a.items.unwrap();
    assert_eq!(
        (i.pitch.q, i.timing.q, i.duration.q),
        (1_000_000, 0, 1_000_000)
    );
    assert_eq!(a.passed, Some(true));
}

/// ST-19、SQ-01：違う音高でもタイミングを採点する。
#[test]
fn st19_wrong_pitch_and_late() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf[1].note = 63;
    shift(&mut perf[1], S / 4);
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (1, 1, 0, 10));
    assert_eq!(a.items.unwrap().timing.denominator, 10);
}

#[test]
fn st20_no_note_off_until_cap() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf[9].off = None;
    let a = run(
        &t1(),
        t,
        &perf,
        RunEnd {
            kind: EndKind::Cap,
            at_ns: 19 * S,
        },
    );
    assert_eq!(counts(&a), (0, 0, 1, 10));
    assert_eq!(a.notes[9].played.unwrap().off_ns, 19 * S);
}

#[test]
fn st21_input_after_the_end_is_dropped() {
    let t = timing(60);
    let mut perf = as_written(&t1(), &t);
    perf.push(P {
        note: 62,
        on: 16 * S + S / 2,
        off: Some(16 * S + 4 * S / 5),
    });
    let a = run(&t1(), t, &perf, normal(16 * S));
    assert_eq!(counts(&a), (0, 0, 0, 10));
    assert!(a.extras.is_empty());
}

#[test]
fn st24_last_note_wrong() {
    let t = timing(60);
    let mut perf = as_written(&t2(), &t);
    perf[3].note = 67;
    assert_eq!(counts(&run(&t2(), t, &perf, normal(8 * S))), (1, 0, 0, 4));
}

#[test]
fn st25_long_rest() {
    let t = timing(60);
    let perf = as_written(&t3(), &t);
    assert_eq!(counts(&run(&t3(), t, &perf, normal(13 * S))), (0, 0, 0, 2));
}

/// ST-26、ST-27、AC-14：弱起の音を採点し、範囲の始まりより前の音は余分にも数えない。
#[test]
fn st26_st27_pickup() {
    let t = timing(60);
    let perf = as_written(&t4(), &t);
    assert_eq!(perf[0].on, 3 * S);
    assert_eq!(counts(&run(&t4(), t, &perf, normal(8 * S))), (0, 0, 0, 2));
    let mut early = perf.clone();
    early.push(P {
        note: 60,
        on: 2 * S,
        off: Some(2 * S + S / 2),
    });
    let a = run(&t4(), t, &early, normal(8 * S));
    assert_eq!(counts(&a), (0, 0, 0, 2));
    assert!(a.extras.is_empty());
}

fn t6_perf(grace_on_ms: Option<(i64, i64)>) -> Vec<P> {
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
    if let Some((on, off)) = grace_on_ms {
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

/// ST-30、SR-19：装飾音は音程とタイミングを採点し、音の長さは採点しない（分母から除く）。
#[test]
fn st30_grace_played_slightly_early() {
    let a = run(&t6(), timing(60), &t6_perf(Some((-60, -10))), normal(8 * S));
    assert_eq!(counts(&a), (0, 0, 0, 4));
    assert_eq!(a.items.unwrap().duration.denominator, 3);
    assert_eq!(a.items.unwrap().timing.denominator, 4);
}

#[test]
fn st31_grace_not_played() {
    let a = run(&t6(), timing(60), &t6_perf(None), normal(8 * S));
    assert_eq!(counts(&a), (1, 0, 0, 3));
    assert_eq!(a.notes[1].pitch, PitchResult::Missed);
}

/// ST-32：6章の限界（装飾音を大きく早く弾くと、抜け＋余分）。
#[test]
fn st32_grace_played_too_early() {
    let a = run(
        &t6(),
        timing(60),
        &t6_perf(Some((-200, -150))),
        normal(8 * S),
    );
    assert_eq!(counts(&a), (2, 0, 0, 3));
}

#[test]
fn st33_last_note_outside_its_window() {
    let t = timing(60);
    let mut perf = as_written(&t2(), &t);
    shift(&mut perf[3], 3 * S / 4);
    let a = run(&t2(), t, &perf, normal(8 * S + 3 * S / 4));
    assert_eq!(counts(&a), (2, 0, 0, 3));
}

#[test]
fn st37_first_press_after_the_cap() {
    let t = timing(60);
    let mut perf = as_written(&t2(), &t);
    perf[3] = P {
        note: 65,
        on: 11 * S + S / 2,
        off: None,
    };
    let a = run(
        &t2(),
        t,
        &perf,
        RunEnd {
            kind: EndKind::Cap,
            at_ns: 11 * S + S / 2,
        },
    );
    assert_eq!(counts(&a), (2, 0, 0, 3));
}

/// AC-14：練習のテンポを変えても、楽譜どおりならタイミングの誤りは 0。換算したテンポ（67.5）でも同じ。
#[test]
fn ac14_other_tempos() {
    for tempo in [Tempo::new(90, 1).unwrap(), Tempo::new(135, 2).unwrap()] {
        let t = Timing {
            tempo,
            origin_ns: 4 * S,
        };
        let perf = as_written(&t1(), &t);
        let end = perf.iter().filter_map(|p| p.off).max().unwrap();
        assert_eq!(counts(&run(&t1(), t, &perf, normal(end))), (0, 0, 0, 10));
    }
}

/// 止まった回は q と合否を出さない（止まった位置の採点は 1a-9）。
#[test]
fn stopped_run_has_no_score() {
    let t = timing(60);
    let perf = as_written(&t2(), &t);
    let a = run(
        &t2(),
        t,
        &perf[..3],
        RunEnd {
            kind: EndKind::Stopped,
            at_ns: 12 * S,
        },
    );
    assert_eq!((a.items, a.passed), (None, None));
}

/// SQ-04：演奏中の仮の印は、その音の窓が閉じた時点までの入力と、窓が閉じた音だけで決める。演奏後の判定とは違ってよい。
#[test]
fn marks_use_only_closed_windows() {
    // 位置 0 ＝ 0 秒。C4 を 1.0 秒と 1.5 秒に（連打）。1.3 秒に1回だけ弾く
    let refs = vec![r(60, (1, 1), (1, 2), false), r(60, (3, 2), (1, 2), false)];
    let t = Timing {
        tempo: Tempo::new(60, 1).unwrap(),
        origin_ns: 0,
    };
    let ev = events(
        &[P {
            note: 60,
            on: 1300 * MS,
            off: Some(1400 * MS),
        }],
        false,
    );
    let input = RunInput {
        reference: &refs,
        timing: t,
        events: &ev,
        rules: &RULES_V0,
    };
    // 1つ目の窓は 1.5 秒、2つ目は 2.0 秒に閉じる
    let first = marks_closed_between(&input, 0, 1500 * MS);
    assert_eq!(first.len(), 1);
    assert_eq!(
        (first[0].ref_index, first[0].pitch, first[0].timing),
        (0, PitchResult::Correct, TimingResult::Late)
    );
    let second = marks_closed_between(&input, 1500 * MS, 2000 * MS);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].ref_index, 1);
    // 演奏後は全体で並べるので、1.3 秒の音は近い2つ目と組になる
    let after = score_run(&input, normal(2 * S));
    assert_eq!(after.notes[0].pitch, PitchResult::Missed);
    assert_eq!(after.notes[1].pitch, PitchResult::Correct);
    assert!(marks_closed_between(&input, 2000 * MS, 9 * S).is_empty());
}
