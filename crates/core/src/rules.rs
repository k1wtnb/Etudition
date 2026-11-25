//! 採点規則の版ごとの名前付きの定数（採点の仕様 v0 の8章。計画 2-3 の5）。
//!
//! 値の正は計画6章で、ここは写し。値を変えるときは版を上げ、新しい定数を足す。
//! 装飾音の扱い（SR-19）もこの版に含める（SR-23）。

use crate::quarters::Quarters;

/// 1つの採点規則の版の値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    /// 採点規則の版の名前。
    pub version: &'static str,
    /// 対応の窓（拍。前後とも）。
    pub match_window: Quarters,
    /// タイミングの許容幅（拍。前後とも）。
    pub timing_tolerance: Quarters,
    /// 演奏の長さ ÷ お手本の長さの下限（分子、分母）。これ未満が誤り。
    pub duration_ratio_min: (i64, i64),
    /// 演奏の長さ ÷ お手本の長さの上限（分子、分母）。これより大きいと誤り。
    pub duration_ratio_max: (i64, i64),
    /// 和音とみなすノートオンの時刻差（拍。これ未満を同じ組にする）。
    pub chord_spread: Quarters,
    /// 音高が違う組の費用（対応の窓の何倍か）。
    pub cost_pitch_mismatch_windows: i64,
    /// 抜けた音・余分な音の費用（対応の窓の何倍か）。
    pub cost_gap_windows: i64,
    /// 止まったの判定の時間（ns）。
    pub stop_timeout_ns: i64,
    /// 終わりの上限（ns）。
    pub end_cap_ns: i64,
    /// 100 点の q。
    pub q_full: u32,
    /// 1 点の q。
    pub q_per_point: u32,
    /// 音程の合格の線（点）。
    pub pass_line_pitch: u32,
    /// 音の長さの合格の線（点）。
    pub pass_line_duration: u32,
    /// 装飾音の音の長さを採点するか（v0 はしない。SR-19）。
    pub grace_duration_scored: bool,
}

/// 採点規則の版 `"v0"`（採点の仕様 v0 の8章。値は仮）。
pub const RULES_V0: Rules = Rules {
    version: "v0",
    match_window: Quarters::new_const(1, 2),
    timing_tolerance: Quarters::new_const(1, 8),
    duration_ratio_min: (1, 2),
    duration_ratio_max: (3, 2),
    chord_spread: Quarters::new_const(1, 8),
    cost_pitch_mismatch_windows: 2,
    cost_gap_windows: 2,
    stop_timeout_ns: 5_000_000_000,
    end_cap_ns: 3_000_000_000,
    q_full: 1_000_000,
    q_per_point: 10_000,
    pass_line_pitch: 90,
    pass_line_duration: 70,
    grace_duration_scored: false,
};
