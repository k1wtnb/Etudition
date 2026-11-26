//! Etudition のアプリ（Tauri の殻）の部品。配線だけを置く（採点と記録は `core` と `engine`）。

pub mod practice;
pub mod ui_time;
pub mod view;

/// アプリを起動する。
pub fn run_app() {
    let builder = tauri::Builder::default().manage(practice::AppState::default());
    // 流し込みのコマンドは、開発用のビルド（feature `dev-feed`）だけに登録する（REQ-106、AC-23）
    #[cfg(feature = "dev-feed")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        ui_time::p0_time_probe,
        practice::load_score,
        practice::start_practice,
        practice::stop_practice,
        practice::feed_midi_file,
    ]);
    #[cfg(not(feature = "dev-feed"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        ui_time::p0_time_probe,
        practice::load_score,
        practice::start_practice,
        practice::stop_practice,
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("Tauri のアプリを起動できなかった");
}
