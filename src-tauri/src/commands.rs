//! The entire Tauri IPC surface.
//!
//! Long-running work (discovery, benchmarks, screenshots, scripts) must not
//! block the UI thread, so those commands run on `spawn_blocking` and report
//! progress through Tauri events.

use std::time::Duration;

use omariglol_core::device::{Device, Target, Transport};
use omariglol_core::screenshot::Screenshot;
use omariglol_core::benchmark::BenchmarkReport;
use serde::{Deserialize, Serialize};

/// Errors surfaced to the frontend.
#[derive(Debug, Serialize)]
pub struct CommandError {
    /// Machine-readable category, so the UI can choose a message.
    pub kind: String,
    pub message: String,
}

pub type CommandResult<T> = std::result::Result<T, CommandError>;

/// The application version, shown in the About panel.
#[tauri::command]
pub fn version() -> String {
    omariglol_core::VERSION.to_string()
}

/// Arguments for [`discover`].
#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    /// Also browse mDNS/DNS-SD.
    pub mdns: bool,
    /// How long to listen, in seconds.
    pub timeout_secs: u64,
}

/// Find instruments on the network.
///
/// Emits `discover:device` events as results arrive, then resolves with the
/// full list, so the UI can populate incrementally.
#[tauri::command]
pub async fn discover(request: DiscoverRequest) -> CommandResult<Vec<Device>> {
    let _ = (request.mdns, Duration::from_secs(request.timeout_secs));
    Err(not_implemented("discover"))
}

/// Arguments for [`connect`].
#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub address: String,
    pub transport: Transport,
    pub port: Option<u16>,
    pub timeout_secs: u64,
}

/// Open a session and return the instrument's identity.
#[tauri::command]
pub async fn connect(request: ConnectRequest) -> CommandResult<String> {
    let _ = (request.address, request.transport, request.port);
    Err(not_implemented("connect"))
}

/// Close the current session.
#[tauri::command]
pub async fn disconnect() -> CommandResult<()> {
    Err(not_implemented("disconnect"))
}

/// Send a SCPI query and return the response.
#[tauri::command]
pub async fn scpi_query(command: String) -> CommandResult<String> {
    let _ = command;
    Err(not_implemented("scpi_query"))
}

/// Send a SCPI command that produces no response.
#[tauri::command]
pub async fn scpi_write(command: String) -> CommandResult<()> {
    let _ = command;
    Err(not_implemented("scpi_write"))
}

/// Arguments for [`capture_screenshot`].
#[derive(Debug, Deserialize)]
pub struct ScreenshotRequest {
    /// Overrides the profile's capture query.
    pub command: Option<String>,
}

/// Capture the instrument's screen.
#[tauri::command]
pub async fn capture_screenshot(request: ScreenshotRequest) -> CommandResult<Screenshot> {
    let _ = request.command;
    Err(not_implemented("capture_screenshot"))
}

/// Arguments for [`run_benchmark`].
#[derive(Debug, Deserialize)]
pub struct BenchmarkRequest {
    pub command: String,
    pub iterations: usize,
}

/// Run a benchmark, emitting `benchmark:sample` events as it goes.
#[tauri::command]
pub async fn run_benchmark(request: BenchmarkRequest) -> CommandResult<BenchmarkReport> {
    let _ = (request.command, request.iterations);
    Err(not_implemented("run_benchmark"))
}

/// Arguments for [`run_script`].
#[derive(Debug, Deserialize)]
pub struct ScriptRequest {
    /// The Python source to execute.
    pub source: String,
    /// Path the source came from, used for tracebacks and relative imports.
    pub path: Option<String>,
}

/// Execute a Python measurement script, emitting `script:event` events.
#[tauri::command]
pub async fn run_script(request: ScriptRequest) -> CommandResult<()> {
    let _ = (request.source, request.path);
    Err(not_implemented("run_script"))
}

/// Ask a running script to stop.
#[tauri::command]
pub async fn cancel_script() -> CommandResult<()> {
    Err(not_implemented("cancel_script"))
}

/// Build the standard "not built yet" error.
///
/// Every command currently returns this. Replacing these bodies is the work of
/// 0.1.0; see `docs/roadmap.md`.
fn not_implemented(what: &str) -> CommandError {
    CommandError {
        kind: "not-implemented".to_string(),
        message: format!("`{what}` is not implemented yet — see docs/roadmap.md"),
    }
}

/// Build a target from a connect request. Kept separate so the conversion is
/// testable independently of the IPC layer.
pub fn target_from_request(request: &ConnectRequest) -> Target {
    let _ = request;
    todo!("resolve the address and build a Target")
}
