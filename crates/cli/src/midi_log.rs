//! 受信を記録する最小のプログラム（計画の F0 と S2b。R03）：midir で受けたメッセージと、`EngineClock` の時刻と
//! midir の生の時刻を組で1行ずつ書く。動かすのは人間で、AI は保存されたログを読む（P01）。

use etudition_engine::input::RawMidi;
use serde_json::{Value, json};

/// 受けたメッセージを JSON の1行にする。
pub fn raw_json(r: &RawMidi) -> Value {
    json!({
        "at_ns": r.at_ns,
        "backend_us": r.backend_us,
        "bytes": r.bytes[..usize::from(r.len.min(3))],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_json_has_both_times_and_the_bytes() {
        let r = RawMidi {
            at_ns: 1_234,
            backend_us: Some(56),
            bytes: [0x90, 60, 100],
            len: 3,
        };
        let v = raw_json(&r);
        assert_eq!(v["at_ns"], 1_234);
        assert_eq!(v["backend_us"], 56);
        assert_eq!(v["bytes"], json!([0x90, 60, 100]));
        let short = RawMidi {
            len: 1,
            bytes: [0xfe, 0, 0],
            ..r
        };
        assert_eq!(raw_json(&short)["bytes"], json!([0xfe]));
    }
}
