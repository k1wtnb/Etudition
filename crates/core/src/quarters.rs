//! 楽譜の上の位置と長さ（単位は四分音符の数）を、整数の分数で持つ。
//!
//! 3連符などで割り切れない値が出るので、浮動小数点は使わない（計画 2-3 の4）。

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, Sub};

/// 四分音符の数を、約分した整数の分数（`num / den`、`den > 0`）で表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Quarters {
    num: i64,
    den: i64,
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Quarters {
    /// 0（1小節目の頭、または長さ 0）。
    pub const ZERO: Quarters = Quarters { num: 0, den: 1 };

    /// `num / den` 個の四分音符を作る。`den` は 0 であってはならない。
    pub fn new(num: i64, den: i64) -> Quarters {
        assert!(den != 0, "分母が 0");
        let sign = if den < 0 { -1 } else { 1 };
        let g = gcd(num, den).max(1);
        Quarters {
            num: sign * num / g,
            den: sign * den / g,
        }
    }

    /// 定数を作る（`const` の文脈用）。約分済みで `den > 0` の値だけを渡す。
    pub const fn new_const(num: i64, den: i64) -> Quarters {
        assert!(den > 0, "分母が正でない");
        Quarters { num, den }
    }

    /// 分子。
    pub fn num(self) -> i64 {
        self.num
    }

    /// 分母（正）。
    pub fn den(self) -> i64 {
        self.den
    }
}

impl Add for Quarters {
    type Output = Quarters;
    fn add(self, o: Quarters) -> Quarters {
        Quarters::new(self.num * o.den + o.num * self.den, self.den * o.den)
    }
}

impl Sub for Quarters {
    type Output = Quarters;
    fn sub(self, o: Quarters) -> Quarters {
        Quarters::new(self.num * o.den - o.num * self.den, self.den * o.den)
    }
}

impl Ord for Quarters {
    fn cmp(&self, o: &Quarters) -> Ordering {
        (i128::from(self.num) * i128::from(o.den)).cmp(&(i128::from(o.num) * i128::from(self.den)))
    }
}

impl PartialOrd for Quarters {
    fn partial_cmp(&self, o: &Quarters) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// 整数なら `3`、そうでなければ `1/3` の形で書く。
impl fmt::Display for Quarters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Quarters;

    #[test]
    fn reduces_and_orders() {
        assert_eq!(Quarters::new(2, 6), Quarters::new(1, 3));
        assert_eq!(Quarters::new(1, -2), Quarters::new(-1, 2));
        assert_eq!(
            Quarters::new(1, 3) + Quarters::new(2, 3),
            Quarters::new(1, 1)
        );
        assert!(Quarters::new(-1, 1) < Quarters::ZERO);
        assert_eq!(Quarters::new(4, 2).to_string(), "2");
        assert_eq!(Quarters::new(-1, 3).to_string(), "-1/3");
    }
}
