//! 採点の中心（`etudition-core`）。
//!
//! 入出力、時計、スレッド、Tauri を持たない。時刻は呼び出す側が渡す。
//! `wasm32-unknown-unknown` 向けにもビルドして、この約束を機械で確かめる。

pub mod expand;
pub mod meter;
pub mod perf;
pub mod quarters;
pub mod rules;
pub mod score;
pub mod scoring;
pub mod timing;
