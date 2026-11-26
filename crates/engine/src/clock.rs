//! 回ごとの時計（境界の約束 B0 の 3-1。B02）。

use std::time::Instant;

/// 回ごとに1つの時計。`t0` は前カウントの最初のクリックの予定時刻で、時刻の原点（0 ns）。
///
/// `Copy` で、コールバックの中でも使える（確保も待ちもしない）。
#[derive(Debug, Clone, Copy)]
pub struct EngineClock {
    t0: Instant,
}

impl EngineClock {
    /// `t0` を原点にする時計を作る。
    pub fn new(t0: Instant) -> EngineClock {
        EngineClock { t0 }
    }

    /// `at` を、原点からの符号付きのナノ秒にする（i64 に収まらないときは端の値）。
    pub fn ns_of(&self, at: Instant) -> i64 {
        let (d, sign) = match at.checked_duration_since(self.t0) {
            Some(d) => (d, 1),
            None => (self.t0.duration_since(at), -1),
        };
        i64::try_from(d.as_nanos()).map_or(if sign > 0 { i64::MAX } else { i64::MIN }, |n| sign * n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn signed_nanoseconds_from_t0() {
        let t0 = Instant::now();
        let c = EngineClock::new(t0);
        assert_eq!(c.ns_of(t0), 0);
        assert_eq!(c.ns_of(t0 + Duration::from_millis(5)), 5_000_000);
        if let Some(before) = t0.checked_sub(Duration::from_millis(3)) {
            assert_eq!(c.ns_of(before), -3_000_000);
        }
    }
}
