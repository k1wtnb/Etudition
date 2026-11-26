//! Etudition のアプリ（Tauri の殻）の入口。

// リリースのビルドで、Windows のコンソールの窓を出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    etudition_app::run_app();
}
