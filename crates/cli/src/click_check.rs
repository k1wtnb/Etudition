//! S2a（クリックの時間軸）の確かめ：cpal でクリックを鳴らし、コールバックごとの時刻の組を記録して、ずれと揺れをまとめる。
//!
//! 鳴らすのは人間が立ち会うとき。音量 0 なら音を出さずに時刻だけを記録する（計画 4章の S2）。

use etudition_engine::click::{CallbackLog, ClickPlan, ClickStamp, PlannedClick};
use serde_json::{Value, json};

/// `secs` 秒のあいだ（0 秒と `secs` 秒を含む）、テンポ `bpm` で鳴らすクリックの数。
pub fn click_count(secs: u32, bpm: u32) -> u32 {
    secs * bpm / 60 + 1
}

/// 4/4 の練習のクリックを `count` 個、`interval_ns` おきに並べる（小節の頭を強く）。
pub fn plan(count: u32, interval_ns: i64) -> ClickPlan {
    ClickPlan {
        clicks: (0..count)
            .map(|i| PlannedClick {
                index: i,
                scheduled_ns: i64::from(i) * interval_ns,
                count_in: i < 4,
                level: if i % 4 == 0 { 2 } else { 1 },
            })
            .collect(),
    }
}

/// コールバックの組を JSON の1行にする（S2a の生の記録）。
pub fn callback_json(l: &CallbackLog) -> Value {
    json!({
        "type": "callback",
        "engine_ns": l.engine_ns,
        "cpal_callback_ns": l.cpal_ts.callback_ns,
        "cpal_playback_ns": l.cpal_ts.playback_ns,
        "frames": l.frames,
    })
}

/// 組の1つを JSON の1行にする。
pub fn stamp_json(s: &ClickStamp) -> Value {
    json!({
        "type": "click",
        "index": s.index,
        "scheduled_ns": s.scheduled_ns,
        "cpal_callback_ns": s.cpal_ts.callback_ns,
        "cpal_playback_ns": s.cpal_ts.playback_ns,
        "at_ns": s.at_ns,
    })
}

/// 鳴った時刻 − 予定時刻（ns）と、再生までの遅れ（再生 − コールバック、ns）の分布。平均だけでなく最小と最大も出す。
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// 組の数。
    pub count: usize,
    /// 鳴った時刻 − 予定時刻：最小、平均、最大（ns）。
    pub offset_ns: (i64, f64, i64),
    /// 再生までの遅れ：最小、平均、最大（ns）。
    pub latency_ns: (i64, f64, i64),
}

fn spread(v: impl Iterator<Item = i64> + Clone) -> (i64, f64, i64) {
    let n = v.clone().count();
    if n == 0 {
        return (0, 0.0, 0);
    }
    let min = v.clone().min().unwrap_or(0);
    let max = v.clone().max().unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let mean = v.map(|x| x as f64).sum::<f64>() / n as f64;
    (min, mean, max)
}

/// 組をまとめる。
pub fn summarize(stamps: &[ClickStamp]) -> Summary {
    Summary {
        count: stamps.len(),
        offset_ns: spread(stamps.iter().map(|s| s.at_ns - s.scheduled_ns)),
        latency_ns: spread(
            stamps
                .iter()
                .map(|s| s.cpal_ts.playback_ns - s.cpal_ts.callback_ns),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use etudition_engine::click::CpalTs;

    fn stamp(index: u32, scheduled_ns: i64, at_ns: i64, latency: i64) -> ClickStamp {
        ClickStamp {
            index,
            scheduled_ns,
            cpal_ts: CpalTs {
                callback_ns: 100,
                playback_ns: 100 + latency,
            },
            at_ns,
        }
    }

    /// 0 秒から指定の秒までのクリックを入れる（1 秒・30 BPM でも 0 秒の1つは鳴らす。Astra の監査）。
    #[test]
    fn plan_count_includes_time_zero() {
        assert_eq!(click_count(1, 30), 1);
        assert_eq!(click_count(10, 120), 21);
    }

    #[test]
    fn plan_is_evenly_spaced_with_downbeats() {
        let p = plan(6, 500_000_000);
        let at: Vec<i64> = p.clicks.iter().map(|c| c.scheduled_ns).collect();
        assert_eq!(
            at,
            vec![
                0,
                500_000_000,
                1_000_000_000,
                1_500_000_000,
                2_000_000_000,
                2_500_000_000
            ]
        );
        let levels: Vec<u8> = p.clicks.iter().map(|c| c.level).collect();
        assert_eq!(levels, vec![2, 1, 1, 1, 2, 1]);
    }

    /// 揺れは平均だけでなく最小と最大も出す（計画 4章の S2「揺れの分布」）。
    #[test]
    fn summary_reports_min_mean_max() {
        let s = summarize(&[
            stamp(0, 0, 1_000_000, 20_000_000),
            stamp(1, 500, 3_000_500, 10_000_000),
            stamp(2, 1000, 2_001_000, 30_000_000),
        ]);
        assert_eq!(s.count, 3);
        assert_eq!(s.offset_ns, (1_000_000, 2_000_000.0, 3_000_000));
        assert_eq!(s.latency_ns, (10_000_000, 20_000_000.0, 30_000_000));
        assert_eq!(summarize(&[]).count, 0);
    }

    #[test]
    fn stamp_json_has_the_pairs() {
        let v = stamp_json(&stamp(4, 2_000, 2_500, 7));
        assert_eq!(v["index"], 4);
        assert_eq!(v["cpal_playback_ns"], 107);
        assert_eq!(v["at_ns"], 2_500);
    }
}
