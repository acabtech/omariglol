//! The Tauri application entry point.
//!
//! `src-tauri` owns the desktop window and nothing else: every instrument-facing
//! operation lives in `omariglol-core`, and everything here is either IPC
//! plumbing or the Omarchy theme bridge.
//!
//! The app is organised around the workspace model (concepts.md §2.4): a
//! [`WorkspaceManager`](omariglol_core::workspace::WorkspaceManager) owns the
//! set of workspaces and a map of live connections holds one session per
//! connected instrument.

pub mod commands;
pub mod theme;

use std::path::PathBuf;

/// The directory workspaces are persisted to.
///
/// Prefers the platform config directory (`~/.config/omariglol/workspaces` on
/// Linux); falls back to the current directory when no home is known, so the
/// app still runs in an unusual environment.
fn workspace_dir() -> PathBuf {
    dirs::config_dir()
        .map(|dir| dir.join("omariglol").join("workspaces"))
        .unwrap_or_else(|| PathBuf::from("omariglol-workspaces"))
}

/// Build and run the desktop application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // The workspace manager, the live connections and the embedded Python
        // engine, shared by every command. Cloning the state is cheap, so
        // background tasks get a copy.
        .manage(commands::WorkspaceState::with_dir(workspace_dir()))
        .invoke_handler(tauri::generate_handler![
            // Version and theme.
            commands::version,
            theme::omarchy_palette,
            // Discovery.
            commands::discover,
            commands::discovery_scan,
            commands::discovery_cache,
            commands::discovery_invalidate,
            commands::network_config,
            // Workspace management.
            commands::workspace_create,
            commands::workspace_list,
            commands::workspace_open,
            commands::workspace_save,
            commands::workspace_delete,
            commands::workspace_get,
            // Instrument management.
            commands::workspace_add_instrument,
            commands::workspace_remove_instrument,
            commands::instrument_list,
            commands::instrument_connect,
            commands::instrument_disconnect,
            commands::instrument_query,
            commands::instrument_write,
            commands::instrument_driver,
            commands::instrument_run_command,
            // Batch and routing.
            commands::instrument_batch_write,
            commands::instrument_batch_query,
            commands::rule_evaluate,
            commands::trigger_arm,
            commands::trigger_disarm,
            commands::trigger_add,
            // Presets and timeline.
            commands::preset_apply,
            commands::timeline_get,
            commands::timeline_scrub,
            // Legacy single-instrument commands.
            commands::connect,
            commands::disconnect,
            commands::scpi_query,
            commands::scpi_write,
            commands::capture_screenshot,
            commands::run_benchmark,
            commands::run_script,
            commands::cancel_script,
        ])
        .run(tauri::generate_context!())
        .expect("error while running omariglol");
}
