//! 楽譜の位置を時刻（ナノ秒）に換算する（採点の仕様 v0 の2章、SR-03〜SR-05）。
//!
//! 時刻の原点（0）は前カウントの最初のクリック（P0-02）。位置 0（1小節目の頭）の時刻は [`Timing::origin_ns`]。

use crate::quarters::Quarters;

/// 1分あたりのナノ秒。
const NS_PER_MINUTE: i128 = 60_000_000_000;

/// 練習のテンポ（四分音符／分）を、約分した正の整数の分数で持つ（境界の約束 B0 の1章）。
///
/// 拍の単位が四分音符でない楽譜は、換算した値をそのまま持つ（例：付点四分音符 45 → 四分音符 67.5 ＝ 135/2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tempo {
    num: u32,
    den: u32,
}

fn gcd(a: u32, b: u32) -> u32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Tempo {
    /// `num / den` 四分音符／分のテンポを作る。どちらかが 0 なら `None`。
    pub fn new(num: u32, den: u32) -> Option<Tempo> {
        if num == 0 || den == 0 {
            return None;
        }
        let g = gcd(num, den);
        Some(Tempo {
            num: num / g,
            den: den / g,
        })
    }

    /// 分子。
    pub fn num(self) -> u32 {
        self.num
    }

    /// 分母。
    pub fn den(self) -> u32 {
        self.den
    }
}

/// 楽譜の位置を時刻にするための組。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// 練習のテンポ。
    pub tempo: Tempo,
    /// 楽譜の位置 0 の時刻（前カウントの終わり。ns）。
    pub origin_ns: i64,
}

/// i128 の値を i64 に収める（ありえない大きさの位置で桁があふれても、パニックしない）。
fn clamp_i64(x: i128) -> i64 {
    i64::try_from(x).unwrap_or(if x < 0 { i64::MIN } else { i64::MAX })
}

/// 拍で書いた幅を ns にする（SR-04。位置 0 の時刻を足さない）。負の数は −∞ の向きへ切り捨てる。
pub fn span_to_ns(tempo: &Tempo, span: Quarters) -> i64 {
    let n = i128::from(span.num()) * NS_PER_MINUTE * i128::from(tempo.den);
    let d = i128::from(span.den()) * i128::from(tempo.num);
    clamp_i64(n.div_euclid(d))
}

/// 楽譜の位置を時刻にする（SR-03）。
pub fn position_to_ns(timing: &Timing, position: Quarters) -> i64 {
    timing
        .origin_ns
        .saturating_add(span_to_ns(&timing.tempo, position))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_is_reduced_and_positive() {
        assert_eq!(
            Tempo::new(135, 2).map(|t| (t.num(), t.den())),
            Some((135, 2))
        );
        assert_eq!(
            Tempo::new(120, 2).map(|t| (t.num(), t.den())),
            Some((60, 1))
        );
        assert_eq!(Tempo::new(0, 1), None);
        assert_eq!(Tempo::new(60, 0), None);
    }

    /// SR-04 の例：テンポ 60 で 1/2 拍 ＝ 500,000,000 ns、1/8 拍 ＝ 125,000,000 ns。
    #[test]
    fn spans_at_tempo_60() {
        let t = Tempo::new(60, 1).unwrap();
        assert_eq!(span_to_ns(&t, Quarters::new(1, 2)), 500_000_000);
        assert_eq!(span_to_ns(&t, Quarters::new(1, 8)), 125_000_000);
    }

    /// 計画 2-3 の1の例：テンポ 60、位置 0 は 4 秒、弱起の位置 −1 は 3 秒。
    #[test]
    fn pickup_is_before_origin() {
        let timing = Timing {
            tempo: Tempo::new(60, 1).unwrap(),
            origin_ns: 4_000_000_000,
        };
        assert_eq!(position_to_ns(&timing, Quarters::new(-1, 1)), 3_000_000_000);
        assert_eq!(position_to_ns(&timing, Quarters::new(8, 1)), 12_000_000_000);
    }

    /// AC-14 の換算：付点四分音符 45（＝四分音符 135/2）で、位置 3/2 は 4/3 秒（切り捨て）。
    /// 負の位置は −∞ の向きへ切り捨てる（div_euclid）。
    #[test]
    fn converted_tempo_and_floor() {
        let timing = Timing {
            tempo: Tempo::new(135, 2).unwrap(),
            origin_ns: 0,
        };
        assert_eq!(position_to_ns(&timing, Quarters::new(3, 2)), 1_333_333_333);
        assert_eq!(
            position_to_ns(&timing, Quarters::new(-3, 2)),
            -1_333_333_334
        );
        let t90 = Tempo::new(90, 1).unwrap();
        assert_eq!(span_to_ns(&t90, Quarters::new(1, 1)), 666_666_666);
    }
}
