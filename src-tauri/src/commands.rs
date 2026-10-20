//! The entire Tauri IPC surface.
//!
//! Long-running work (discovery, benchmarks, screenshots, scripts) must not
//! block the UI thread, so those commands run on `spawn_blocking` and report
//! progress through Tauri events.
//!
//! ## State
//!
//! The app holds exactly one active [`Session`] and one embedded Python
//! [`ScriptEngine`], both behind mutexes. [`AppState`] is cheap to clone (every
//! field is an `Arc`), so a clone can be moved into a background task while the
//! managed copy stays put.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use omariglol_core::benchmark::{self, BenchmarkOptions, BenchmarkReport};
use omariglol_core::device::{Device, Target, Transport};
use omariglol_core::discovery;
use omariglol_core::error::Error;
use omariglol_core::screenshot::{self, ImageFormat, Screenshot};
use omariglol_core::transport::{self, Session};
use omariglol_script::{ScriptEngine, ScriptOptions};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

/// Event emitted as each instrument is discovered.
pub const EVENT_DISCOVER_DEVICE: &str = "discover:device";
/// Event emitted after each benchmark round trip.
pub const EVENT_BENCHMARK_SAMPLE: &str = "benchmark:sample";
/// Event emitted for each line, sample or status a script produces.
pub const EVENT_SCRIPT_EVENT: &str = "script:event";

/// Errors surfaced to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    /// Machine-readable category, so the UI can choose a message.
    pub kind: String,
    pub message: String,
}

impl CommandError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        CommandError {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

pub type CommandResult<T> = std::result::Result<T, CommandError>;

/// Map a core error onto a machine-readable `kind` plus a message.
///
/// Timeouts are classified first, so an OS-level `TimedOut` I/O error is
/// reported as `timeout` rather than the generic `io`.
impl From<Error> for CommandError {
    fn from(err: Error) -> Self {
        let kind = if err.is_timeout() {
            "timeout"
        } else {
            match &err {
                Error::Io(_) => "io",
                Error::Addr(_) => "addr",
                Error::Timeout(_) => "timeout",
                Error::Vxi11 { .. } => "vxi11",
                Error::Rpc(_) => "rpc",
                Error::Portmap { .. } => "portmap",
                Error::NotFound(_) => "not-found",
                Error::Malformed(_) => "malformed",
                Error::ScreenshotUnsupported(_) => "screenshot-unsupported",
                Error::Unsupported(_) => "unsupported",
                Error::Other(_) => "other",
            }
        };
        CommandError::new(kind, err.to_string())
    }
}

/// Map a scripting error onto a machine-readable `kind` plus a message.
impl From<omariglol_script::ScriptError> for CommandError {
    fn from(err: omariglol_script::ScriptError) -> Self {
        use omariglol_script::ScriptError;
        let kind = match &err {
            ScriptError::Read { .. } => "io",
            ScriptError::Python { .. } => "python",
            ScriptError::Cancelled => "cancelled",
            ScriptError::Timeout(_) => "timeout",
            ScriptError::InterpreterUnavailable(_) => "interpreter",
            ScriptError::Other(_) => "other",
        };
        CommandError::new(kind, err.to_string())
    }
}

/// Wrap a background-task join failure.
fn join_error(err: impl std::fmt::Display) -> CommandError {
    CommandError::new("internal", format!("background task failed: {err}"))
}

/// The one active session, plus the identity captured when it was opened.
struct ActiveSession {
    session: Session,
    idn: String,
}

/// Shared application state.
///
/// Cloning is cheap: every field is an `Arc`, so a clone can be moved into a
/// `spawn_blocking` task while the managed copy stays put.
#[derive(Clone)]
pub struct AppState {
    session: Arc<Mutex<Option<ActiveSession>>>,
    engine: Arc<Mutex<Option<ScriptEngine>>>,
    script_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            session: Arc::new(Mutex::new(None)),
            engine: Arc::new(Mutex::new(None)),
            script_cancel: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// The standard "no instrument connected" error.
fn no_session() -> CommandError {
    CommandError::new(
        "no-session",
        "No instrument connected. Open one from Search.",
    )
}

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
pub async fn discover(app: AppHandle, request: DiscoverRequest) -> CommandResult<Vec<Device>> {
    let timeout = Duration::from_secs(request.timeout_secs);
    let mdns = request.mdns;
    tauri::async_runtime::spawn_blocking(move || {
        // Run the two mechanisms separately so each result can be emitted the
        // moment it is found, then merge them the way `discover` would.
        let mut devices = discovery::discover_vxi11(timeout).unwrap_or_default();
        for device in &devices {
            let _ = app.emit(EVENT_DISCOVER_DEVICE, device.clone());
        }
        if mdns && let Ok(found) = discovery::discover_mdns(timeout) {
            for device in &found {
                let _ = app.emit(EVENT_DISCOVER_DEVICE, device.clone());
            }
            devices = discovery::merge_devices(devices, found);
        }
        Ok(devices)
    })
    .await
    .map_err(join_error)?
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
pub async fn connect(state: State<'_, AppState>, request: ConnectRequest) -> CommandResult<String> {
    let state = state.inner().clone();
    let target = target_from_request(&request)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut session = Session::open(target)?;
        let idn = session.identify()?;
        *state.session.lock().unwrap() = Some(ActiveSession {
            session,
            idn: idn.clone(),
        });
        Ok(idn)
    })
    .await
    .map_err(join_error)?
}

/// Close the current session.
#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> CommandResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(mut active) = state.session.lock().unwrap().take() {
            let _ = active.session.close();
        }
        Ok(())
    })
    .await
    .map_err(join_error)?
}

/// Send a SCPI query and return the response.
#[tauri::command]
pub async fn scpi_query(state: State<'_, AppState>, command: String) -> CommandResult<String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.session.lock().unwrap();
        let active = guard.as_mut().ok_or_else(no_session)?;
        Ok(active.session.query(&command)?)
    })
    .await
    .map_err(join_error)?
}

/// Send a SCPI command that produces no response.
#[tauri::command]
pub async fn scpi_write(state: State<'_, AppState>, command: String) -> CommandResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.session.lock().unwrap();
        let active = guard.as_mut().ok_or_else(no_session)?;
        active.session.send(&command)?;
        Ok(())
    })
    .await
    .map_err(join_error)?
}

/// Arguments for [`capture_screenshot`].
#[derive(Debug, Deserialize)]
pub struct ScreenshotRequest {
    /// Overrides the profile's capture query.
    pub command: Option<String>,
}

/// A screenshot ready for the frontend: the image is base64-encoded.
///
/// The core [`Screenshot`] skips its raw bytes when serialised, so the IPC
/// boundary carries the base64 form the frontend embeds directly.
#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotResponse {
    /// Base64-encoded image bytes.
    pub data: String,
    pub format: ImageFormat,
    pub idn: String,
    pub profile: String,
}

impl From<Screenshot> for ScreenshotResponse {
    fn from(shot: Screenshot) -> Self {
        ScreenshotResponse {
            data: shot.to_base64(),
            format: shot.format,
            idn: shot.idn,
            profile: shot.profile,
        }
    }
}

/// Capture the instrument's screen.
///
/// Image acquisition opens its own raw transport (the core's default for
/// screenshots, which streams at TCP speed) using the active session's address
/// and timeout.
#[tauri::command]
pub async fn capture_screenshot(
    state: State<'_, AppState>,
    request: ScreenshotRequest,
) -> CommandResult<ScreenshotResponse> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (address, timeout, idn) = {
            let guard = state.session.lock().unwrap();
            let active = guard.as_ref().ok_or_else(no_session)?;
            (
                active.session.target.address,
                active.session.target.timeout,
                active.idn.clone(),
            )
        };
        let mut transport = screenshot::open_default_transport(address, timeout)?;
        let shot = screenshot::capture(transport.as_mut(), &idn, request.command.as_deref())?;
        Ok(ScreenshotResponse::from(shot))
    })
    .await
    .map_err(join_error)?
}

/// Arguments for [`run_benchmark`].
#[derive(Debug, Deserialize)]
pub struct BenchmarkRequest {
    pub command: String,
    pub iterations: usize,
}

/// One benchmark progress sample, emitted as `benchmark:sample`.
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkSample {
    pub iteration: usize,
    pub sample_ms: f64,
}

/// Run a benchmark, emitting `benchmark:sample` events as it goes.
#[tauri::command]
pub async fn run_benchmark(
    app: AppHandle,
    state: State<'_, AppState>,
    request: BenchmarkRequest,
) -> CommandResult<BenchmarkReport> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.session.lock().unwrap();
        let active = guard.as_mut().ok_or_else(no_session)?;
        let options = BenchmarkOptions {
            command: request.command,
            iterations: request.iterations,
            ..Default::default()
        };
        let report = benchmark::run(
            active.session.transport_mut(),
            &options,
            |iteration, sample_ms| {
                let _ = app.emit(
                    EVENT_BENCHMARK_SAMPLE,
                    BenchmarkSample {
                        iteration,
                        sample_ms,
                    },
                );
            },
        )?;
        Ok(report)
    })
    .await
    .map_err(join_error)?
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
pub async fn run_script(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ScriptRequest,
) -> CommandResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // The engine lock is held for the whole run; `cancel_script` uses the
        // separate cancel handle instead, so it never waits on this lock.
        let mut guard = state.engine.lock().unwrap();
        if guard.is_none() {
            *guard = Some(ScriptEngine::new()?);
        }
        let engine = guard.as_mut().expect("engine initialised above");
        *state.script_cancel.lock().unwrap() = Some(engine.cancel_handle());

        let options = ScriptOptions {
            working_dir: request
                .path
                .as_deref()
                .and_then(|path| Path::new(path).parent().map(Path::to_path_buf)),
            ..Default::default()
        };

        let result = engine.run_source(&request.source, &options, move |event| {
            let _ = app.emit(EVENT_SCRIPT_EVENT, event);
        });

        *state.script_cancel.lock().unwrap() = None;
        result?;
        Ok(())
    })
    .await
    .map_err(join_error)?
}

/// Ask a running script to stop.
#[tauri::command]
pub async fn cancel_script(state: State<'_, AppState>) -> CommandResult<()> {
    if let Some(flag) = state.script_cancel.lock().unwrap().clone() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Build a target from a connect request. Kept separate so the conversion is
/// testable independently of the IPC layer.
pub fn target_from_request(request: &ConnectRequest) -> omariglol_core::Result<Target> {
    let address = transport::parse_address(&request.address)?;
    let mut target = Target::new(address, request.transport)
        .with_timeout(Duration::from_secs(request.timeout_secs));
    if let Some(port) = request.port {
        target = target.with_port(port);
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn request(
        address: &str,
        transport: Transport,
        port: Option<u16>,
        timeout_secs: u64,
    ) -> ConnectRequest {
        ConnectRequest {
            address: address.to_string(),
            transport,
            port,
            timeout_secs,
        }
    }

    #[test]
    fn test_target_from_request_literal_ip() {
        let target =
            target_from_request(&request("192.168.1.50", Transport::Raw, Some(5555), 7)).unwrap();
        assert_eq!(target.address, "192.168.1.50".parse::<IpAddr>().unwrap());
        assert_eq!(target.transport, Transport::Raw);
        assert_eq!(target.port, Some(5555));
        assert_eq!(target.timeout, Duration::from_secs(7));
    }

    #[test]
    fn test_target_from_request_defaults_port() {
        let target = target_from_request(&request("10.0.0.1", Transport::Vxi11, None, 3)).unwrap();
        assert_eq!(target.port, None);
        // VXI-11 resolves through the portmapper.
        assert_eq!(target.effective_port(), 111);
    }

    #[test]
    fn test_target_from_request_rejects_empty_address() {
        assert!(target_from_request(&request("   ", Transport::Raw, None, 3)).is_err());
    }

    #[test]
    fn test_command_error_kind_from_core_error() {
        assert_eq!(
            CommandError::from(Error::Timeout("read".to_string())).kind,
            "timeout"
        );
        assert_eq!(
            CommandError::from(Error::Addr("bad".to_string())).kind,
            "addr"
        );
        assert_eq!(
            CommandError::from(Error::ScreenshotUnsupported("x".to_string())).kind,
            "screenshot-unsupported"
        );
        assert_eq!(
            CommandError::from(Error::NotFound("127.0.0.1:5025".parse().unwrap())).kind,
            "not-found"
        );
    }

    #[test]
    fn test_command_error_kind_from_script_error() {
        use omariglol_script::ScriptError;
        assert_eq!(CommandError::from(ScriptError::Cancelled).kind, "cancelled");
        assert_eq!(
            CommandError::from(ScriptError::Timeout(Duration::from_secs(1))).kind,
            "timeout"
        );
        assert_eq!(
            CommandError::from(ScriptError::Python {
                traceback: "boom".to_string()
            })
            .kind,
            "python"
        );
    }
}
