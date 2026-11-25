//! Etudition のアプリ（Tauri の殻）。配線だけを置く。

// リリースのビルドで、Windows のコンソールの窓を出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ui_time;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ui_time::p0_time_probe])
        .run(tauri::generate_context!())
        .expect("Tauri のアプリを起動できなかった");
}
