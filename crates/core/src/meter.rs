//! 拍子から、クリック、音楽の拍、1小節の長さを決める（Q-29。人間の回答 2026-11-26）。
//!
//! - クリック：分母の音符ごとに、1小節に分子の数だけ鳴らす（6/8 は八分音符で6回、2/2 は二分音符で2回）。
//! - 音楽の拍（テンポの表示と、採点のタイミングの許容幅の単位）：複合拍子（分子が 6・9・12 など3の倍数で6以上）は
//!   付点の音符（分母の音符3つ。6/8 は付点四分音符）、それ以外は分母の音符。
//! - 拍子は弱起でない最初の小節のものを使う（曲の途中の拍子の変化は後の段階。仕様書7章の W11）。

use crate::quarters::Quarters;
use crate::score::Score;

/// 拍子（分子、分母）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meter {
    /// 分子。
    pub beats: i64,
    /// 分母。
    pub beat_type: i64,
}

impl Meter {
    /// 楽譜の、弱起でない最初の小節の拍子。拍子がない、または正でなければ 4/4。
    pub fn of(score: &Score) -> Meter {
        score
            .measures
            .iter()
            .find(|m| !m.implicit)
            .map(|m| Meter {
                beats: m.time_beats,
                beat_type: m.time_beat_type,
            })
            .filter(|m| m.beats > 0 && m.beat_type > 0)
            .unwrap_or(Meter {
                beats: 4,
                beat_type: 4,
            })
    }

    /// 1小節の長さ（四分音符の数）。前カウントの長さ（楽譜の位置 0 までの時間）に使う。
    pub fn measure(self) -> Quarters {
        Quarters::new(self.beats * 4, self.beat_type)
    }

    /// 1小節のクリックの数（分子）。
    pub fn clicks(self) -> u32 {
        u32::try_from(self.beats).unwrap_or(4)
    }

    /// クリックの間隔（分母の音符。四分音符の数）。
    pub fn click_unit(self) -> Quarters {
        Quarters::new(4, self.beat_type)
    }

    /// 複合拍子か（分子が3の倍数で6以上）。
    pub fn is_compound(self) -> bool {
        self.beats >= 6 && self.beats % 3 == 0
    }

    /// 音楽の拍（四分音符の数）。
    pub fn beat(self) -> Quarters {
        if self.is_compound() {
            Quarters::new(12, self.beat_type)
        } else {
            Quarters::new(4, self.beat_type)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::read_score;

    fn meter(beats: i64, beat_type: i64) -> Meter {
        Meter { beats, beat_type }
    }

    #[test]
    fn clicks_and_beats_follow_q29() {
        // (拍子, クリックの数, クリックの間隔, 音楽の拍, 1小節)
        let cases = [
            (
                (4, 4),
                4,
                Quarters::new(1, 1),
                Quarters::new(1, 1),
                Quarters::new(4, 1),
            ),
            (
                (3, 4),
                3,
                Quarters::new(1, 1),
                Quarters::new(1, 1),
                Quarters::new(3, 1),
            ),
            (
                (2, 4),
                2,
                Quarters::new(1, 1),
                Quarters::new(1, 1),
                Quarters::new(2, 1),
            ),
            (
                (6, 8),
                6,
                Quarters::new(1, 2),
                Quarters::new(3, 2),
                Quarters::new(3, 1),
            ),
            (
                (9, 8),
                9,
                Quarters::new(1, 2),
                Quarters::new(3, 2),
                Quarters::new(9, 2),
            ),
            (
                (12, 8),
                12,
                Quarters::new(1, 2),
                Quarters::new(3, 2),
                Quarters::new(6, 1),
            ),
            (
                (2, 2),
                2,
                Quarters::new(2, 1),
                Quarters::new(2, 1),
                Quarters::new(4, 1),
            ),
            (
                (3, 8),
                3,
                Quarters::new(1, 2),
                Quarters::new(1, 2),
                Quarters::new(3, 2),
            ),
            (
                (6, 4),
                6,
                Quarters::new(1, 1),
                Quarters::new(3, 1),
                Quarters::new(6, 1),
            ),
        ];
        for ((b, t), clicks, unit, beat, measure) in cases {
            let m = meter(b, t);
            assert_eq!(m.clicks(), clicks, "{b}/{t}");
            assert_eq!(m.click_unit(), unit, "{b}/{t}");
            assert_eq!(m.beat(), beat, "{b}/{t}");
            assert_eq!(m.measure(), measure, "{b}/{t}");
            // クリックの間隔 × 数 ＝ 1小節（人間の回答「1小節分に相当する回数」）
            assert_eq!(
                Quarters::new(unit.num() * i64::from(clicks), unit.den()),
                measure,
                "{b}/{t}"
            );
        }
    }

    #[test]
    fn meter_is_read_from_the_first_full_measure() {
        let xml = |time: &str| {
            format!(
                r#"<?xml version="1.0"?><score-partwise><part-list/><part id="P1"><measure number="1"><attributes><divisions>2</divisions>{time}</attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>6</duration></note></measure></part></score-partwise>"#
            )
        };
        let six =
            read_score(xml("<time><beats>6</beats><beat-type>8</beat-type></time>").as_bytes())
                .unwrap();
        assert_eq!(Meter::of(&six.score), meter(6, 8));
        // 拍子がなければ 4/4
        let none = read_score(xml("").as_bytes()).unwrap();
        assert_eq!(Meter::of(&none.score), meter(4, 4));
    }
}
