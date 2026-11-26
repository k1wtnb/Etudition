//! 1a-5 のクリック（境界の約束 B0 の 3-3）：予定時刻の列、偽の音声出力、時刻の組から鳴った時刻を求める計算。

use etudition_core::expand::expand;
use etudition_core::meter::Meter;
use etudition_core::score::read_score;
use etudition_core::timing::{Tempo, Timing};
use etudition_engine::click::{AudioMsg, AudioOut, ClickPlan, CpalTs, FakeOut, placed_at};
use etudition_engine::clock::EngineClock;
use etudition_engine::input::StartError;
use std::time::Instant;

const S: i64 = 1_000_000_000;

/// 1小節の楽譜（拍子と、全部の長さの音1つ）。
fn one_measure(
    beats: u32,
    beat_type: u32,
    divisions: u32,
    duration: u32,
) -> etudition_core::expand::Expansion {
    let xml = format!(
        r#"<?xml version="1.0"?><score-partwise><part-list/><part id="P1"><measure number="1"><attributes><divisions>{divisions}</divisions><time><beats>{beats}</beats><beat-type>{beat_type}</beat-type></time></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>{duration}</duration></note></measure></part></score-partwise>"#
    );
    expand(&read_score(xml.as_bytes()).unwrap().score)
}

fn timing(bpm: u32, origin_ns: i64) -> Timing {
    Timing {
        tempo: Tempo::new(bpm, 1).unwrap(),
        origin_ns,
    }
}

/// AC-47 と Q-29：前カウントは1小節分（分母の音符ごとに分子の数）。曲の途中も同じ間隔で、最後の音の終わりの前まで鳴らす。
#[test]
fn plan_counts_in_one_measure_and_continues_through_the_piece() {
    // 4/4、♩＝60：前カウントは 0、1、2、3 秒。位置 0 は 4 秒。曲の間は 4〜7 秒
    let e = one_measure(4, 4, 1, 4);
    let p = ClickPlan::new(
        &timing(60, 4 * S),
        Meter {
            beats: 4,
            beat_type: 4,
        },
        &e,
    );
    let at: Vec<i64> = p.clicks.iter().map(|c| c.scheduled_ns).collect();
    assert_eq!(at, vec![0, S, 2 * S, 3 * S, 4 * S, 5 * S, 6 * S, 7 * S]);
    let count_in: Vec<bool> = p.clicks.iter().map(|c| c.count_in).collect();
    assert_eq!(
        count_in,
        vec![true, true, true, true, false, false, false, false]
    );
    let index: Vec<u32> = p.clicks.iter().map(|c| c.index).collect();
    assert_eq!(index, (0..8).collect::<Vec<u32>>());

    // 6/8、♩＝60（♩.＝40）：八分音符で6回、0.5 秒おき。位置 0 は 3 秒
    let e = one_measure(6, 8, 2, 6);
    let p = ClickPlan::new(
        &timing(60, 3 * S),
        Meter {
            beats: 6,
            beat_type: 8,
        },
        &e,
    );
    let first: Vec<i64> = p.clicks.iter().take(7).map(|c| c.scheduled_ns).collect();
    assert_eq!(first, vec![0, S / 2, S, 3 * S / 2, 2 * S, 5 * S / 2, 3 * S]);
    assert_eq!(p.clicks.iter().filter(|c| c.count_in).count(), 6);
    assert_eq!(p.clicks.len(), 12);
}

/// 練習の助けに、小節の頭を強く、音楽の拍の頭を中くらい、ほかを弱く鳴らす（6/8 は1つ目と4つ目が拍の頭）。
#[test]
fn plan_marks_the_downbeat_and_the_musical_beats() {
    let e = one_measure(6, 8, 2, 6);
    let p = ClickPlan::new(
        &timing(60, 3 * S),
        Meter {
            beats: 6,
            beat_type: 8,
        },
        &e,
    );
    let levels: Vec<u8> = p.clicks.iter().take(6).map(|c| c.level).collect();
    assert_eq!(levels, vec![2, 0, 0, 1, 0, 0]);
    let e = one_measure(4, 4, 1, 4);
    let p = ClickPlan::new(
        &timing(60, 4 * S),
        Meter {
            beats: 4,
            beat_type: 4,
        },
        &e,
    );
    let levels: Vec<u8> = p.clicks.iter().take(4).map(|c| c.level).collect();
    assert_eq!(levels, vec![2, 1, 1, 1]);
}

/// AC-15 の計算：コールバックを呼んだときの時計の時刻と、cpal のコールバックと再生の時刻の組から、鳴った時刻を求める。
#[test]
fn placed_time_is_callback_time_plus_output_latency() {
    let ts = CpalTs {
        callback_ns: 10_000_000_000,
        playback_ns: 10_020_000_000,
    };
    // 時計の 3.000 秒に呼ばれ、20 ms 後に再生されるバッファの、48 kHz の 480 フレーム目（10 ms）
    assert_eq!(
        placed_at(3 * S, ts, 480, 48_000),
        3 * S + 20_000_000 + 10_000_000
    );
    // 再生の時刻がコールバックより前（裏側の誤り）なら、遅れは 0 とみなす
    let back = CpalTs {
        callback_ns: 10,
        playback_ns: 5,
    };
    assert_eq!(placed_at(S, back, 0, 48_000), S);
}

/// AC-15：偽の出力は、決めた cpal の時刻と遅れで、計画のクリックごとに時刻の組を送る。鳴った時刻は予定時刻と同じになる。
#[test]
fn fake_out_stamps_every_click_with_known_cpal_times() {
    let e = one_measure(4, 4, 1, 4);
    let p = ClickPlan::new(
        &timing(60, 4 * S),
        Meter {
            beats: 4,
            beat_type: 4,
        },
        &e,
    );
    let (tx, mut rx) = rtrb::RingBuffer::new(64);
    let mut out = FakeOut::new(5 * S, 20_000_000);
    out.start(EngineClock::new(Instant::now()), &p, tx).unwrap();
    let mut stamps = Vec::new();
    while let Ok(m) = rx.pop() {
        match m {
            AudioMsg::Stamp(s) => stamps.push(s),
            AudioMsg::Lost => panic!("失わないはず"),
        }
    }
    assert_eq!(stamps.len(), p.clicks.len());
    let s = stamps[1];
    assert_eq!((s.index, s.scheduled_ns), (1, S));
    // コールバックは予定の 20 ms 前、再生は予定ちょうど（cpal の時刻は 5 秒ずれた軸）
    assert_eq!(s.cpal_ts.callback_ns, 5 * S + S - 20_000_000);
    assert_eq!(s.cpal_ts.playback_ns, 5 * S + S);
    assert_eq!(s.at_ns, S);
    out.stop();
}

/// AC-46：出力先がなければ始められない。途中で失えば、その後に `Lost` が届く。
#[test]
fn fake_out_can_be_missing_or_lost() {
    let e = one_measure(4, 4, 1, 4);
    let p = ClickPlan::new(
        &timing(60, 4 * S),
        Meter {
            beats: 4,
            beat_type: 4,
        },
        &e,
    );
    let (tx, _rx) = rtrb::RingBuffer::new(64);
    let mut none = FakeOut::missing();
    assert_eq!(
        none.start(EngineClock::new(Instant::now()), &p, tx),
        Err(StartError::NoAudioOutput)
    );

    let (tx, mut rx) = rtrb::RingBuffer::new(64);
    let mut lost = FakeOut::new(0, 0).lost_after(2);
    lost.start(EngineClock::new(Instant::now()), &p, tx)
        .unwrap();
    let mut got = Vec::new();
    while let Ok(m) = rx.pop() {
        got.push(m);
    }
    assert_eq!(got.len(), 3);
    assert!(matches!(got[0], AudioMsg::Stamp(_)));
    assert!(matches!(got[1], AudioMsg::Stamp(_)));
    assert_eq!(got[2], AudioMsg::Lost);
}
