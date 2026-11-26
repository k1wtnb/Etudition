//! Etudition のアプリ（Tauri の殻）の部品。配線だけを置く（採点と記録は `core` と `engine`）。

pub mod practice;
pub mod ui_time;
pub mod view;

/// 状態とコマンドを登録する。流し込みのコマンドは、開発用のビルド（feature `dev-feed`）だけに登録する（REQ-106、AC-23）。
pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    let builder = builder.manage(practice::AppState::default());
    #[cfg(feature = "dev-feed")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        practice::load_score,
        practice::start_practice,
        practice::stop_practice,
        practice::feed_midi_file,
    ]);
    #[cfg(not(feature = "dev-feed"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        practice::load_score,
        practice::start_practice,
        practice::stop_practice,
    ]);
    builder
}

/// アプリを起動する。
pub fn run_app() {
    register(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("Tauri のアプリを起動できなかった");
}
