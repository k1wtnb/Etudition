//! 画面に渡す時刻の形（P0-02、人間が 2026-11-25 に案A で決めた。計画 2-3 の2）。
//!
//! - 原点：前カウントの最初のクリックを 0 ns とする
//! - 単位と型：符号付きのナノ秒。Rust では i64、画面（TypeScript）では number
//! - 範囲：JavaScript の number で正確に表せる ±(2^53 − 1) の中だけ。外なら丸めずに誤りにする

use serde::Serialize;
use ts_rs::TS;

/// 画面に渡せる時刻の最大の絶対値（2^53 − 1 ns。約104日）。
pub const MAX_UI_NS: i64 = (1_i64 << 53) - 1;

/// 画面に渡す時刻（前カウントの最初のクリックを 0 とする、符号付きのナノ秒）。
/// 範囲の確かめを必ず通すため、値は `UiTime::new` でだけ作れる。
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct UiTime {
    #[ts(type = "number")]
    ns: i64,
}

impl UiTime {
    /// 画面に渡せる範囲（±MAX_UI_NS）の中なら作る。外なら誤りを返す（丸めない）。
    pub fn new(ns: i64) -> Result<Self, String> {
        if (-MAX_UI_NS..=MAX_UI_NS).contains(&ns) {
            Ok(Self { ns })
        } else {
            Err(format!(
                "画面に渡せる時刻の範囲（±{MAX_UI_NS} ns）の外：{ns}"
            ))
        }
    }
}

/// P0 の④の確かめ：代表値（範囲の上限）を画面に返す。画面が number として同じ値を受け取るかを見る。
#[tauri::command]
pub fn p0_time_probe() -> Result<UiTime, String> {
    UiTime::new(MAX_UI_NS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_type_is_number() {
        let decl = UiTime::decl(&ts_rs::Config::default());
        assert!(decl.contains("ns: number"), "{decl}");
    }

    #[test]
    fn accepts_values_within_safe_integer_range() {
        assert_eq!(UiTime::new(0).unwrap().ns, 0);
        assert_eq!(UiTime::new(MAX_UI_NS).unwrap().ns, MAX_UI_NS);
        assert_eq!(UiTime::new(-MAX_UI_NS).unwrap().ns, -MAX_UI_NS);
    }

    #[test]
    fn rejects_values_outside_safe_integer_range() {
        assert!(UiTime::new(MAX_UI_NS + 1).is_err());
        assert!(UiTime::new(-MAX_UI_NS - 1).is_err());
        assert!(UiTime::new(i64::MIN).is_err());
        assert!(UiTime::new(i64::MAX).is_err());
    }

    #[test]
    fn json_has_exact_integer() {
        let json = serde_json::to_string(&p0_time_probe().unwrap()).unwrap();
        assert_eq!(json, r#"{"ns":9007199254740991}"#);
    }
}
