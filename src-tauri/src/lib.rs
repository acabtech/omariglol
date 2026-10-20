//! The Tauri application entry point.
//!
//! `src-tauri` owns the desktop window and nothing else: every instrument-facing
//! operation lives in `omariglol-core`, and everything here is either IPC
//! plumbing or the Omarchy theme bridge.

pub mod commands;
pub mod theme;

/// Build and run the desktop application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::version,
            commands::discover,
            commands::connect,
            commands::disconnect,
            commands::scpi_query,
            commands::scpi_write,
            commands::capture_screenshot,
            commands::run_benchmark,
            commands::run_script,
            commands::cancel_script,
            theme::omarchy_palette,
        ])
        .run(tauri::generate_context!())
        .expect("error while running omariglol");
}
