//! MIDI、クリック、時計、記録、流し込み（`etudition-engine`）。
//!
//! Tauri を持たない。時計はここに1本だけ置く。名前と形は境界の約束（B0）の3章。

pub mod clock;
pub mod input;
pub mod record;
pub mod session;
