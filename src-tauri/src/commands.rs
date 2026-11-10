//! The entire Tauri IPC surface.
//!
//! Long-running work (discovery, benchmarks, screenshots, scripts) must not
//! block the UI thread, so those commands run on `spawn_blocking` and report
//! progress through Tauri events.
//!
//! ## State
//!
//! The app is organised around the **workspace** model (concepts.md §2.4): a
//! [`WorkspaceManager`] owns the set of workspaces and tracks the active one,
//! and a map of live [`Connection`]s holds one transport session per connected
//! instrument. A single embedded Python [`ScriptEngine`] is shared.
//!
//! [`WorkspaceState`] is cheap to clone (every field is an `Arc`), so a clone
//! can be moved into a background task while the managed copy stays put.
//!
//! ## Locking
//!
//! The manager and the session map are separate mutexes. Commands never hold
//! both at once: they read what they need from the manager, release it, do the
//! network work under the session lock, then re-acquire the manager to record
//! the result. This keeps the lock order trivial and deadlock-free.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use omariglol_core::benchmark::{self, BenchmarkOptions, BenchmarkReport};
use omariglol_core::connection::Connection;
use omariglol_core::device::{Device, Target, Transport};
use omariglol_core::discovery;
use omariglol_core::discovery_engine::DEFAULT_TTL_SECS;
use omariglol_core::discovery_engine::{
    CachedInstrument, DiscoveryEngine, DiscoveryEvent, DiscoveryOptions, DiscoveryReport,
};
use omariglol_core::driver::InstrumentDriver;
use omariglol_core::error::Error;
use omariglol_core::instrument::{Instrument, InstrumentId, InstrumentState};
use omariglol_core::netconfig::{self, NetworkConfig};
use omariglol_core::preset::Preset;
use omariglol_core::rule::Rule;
use omariglol_core::screenshot::{self, ImageFormat, Screenshot};
use omariglol_core::timeline::{TimelineEvent, TimelineEventKind};
use omariglol_core::trigger::Trigger;
use omariglol_core::workspace::{Workspace, WorkspaceId, WorkspaceManager, now_millis};
use omariglol_script::{ScriptEngine, ScriptOptions};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Event emitted as each instrument is discovered.
pub const EVENT_DISCOVER_DEVICE: &str = "discover:device";
/// Event emitted after each benchmark round trip.
pub const EVENT_BENCHMARK_SAMPLE: &str = "benchmark:sample";
/// Event emitted for each line, sample or status a script produces.
pub const EVENT_SCRIPT_EVENT: &str = "script:event";

/// An instrument in the workspace connected successfully.
pub const EVENT_WORKSPACE_INSTRUMENT_CONNECTED: &str = "workspace:instrument-connected";
/// An instrument in the workspace disconnected.
pub const EVENT_WORKSPACE_INSTRUMENT_DISCONNECTED: &str = "workspace:instrument-disconnected";
/// An instrument reported an error.
pub const EVENT_WORKSPACE_INSTRUMENT_ERROR: &str = "workspace:instrument-error";
/// An instrument's state changed.
pub const EVENT_WORKSPACE_INSTRUMENT_STATE: &str = "workspace:instrument-state";
/// A conditional routing rule was evaluated and its action executed.
pub const EVENT_WORKSPACE_RULE_FIRED: &str = "workspace:rule-fired";
/// A trigger condition was met and its action executed.
pub const EVENT_WORKSPACE_TRIGGER_FIRED: &str = "workspace:trigger-fired";
/// A new event was added to the shared timeline.
pub const EVENT_WORKSPACE_TIMELINE_EVENT: &str = "workspace:timeline-event";
/// A preset was applied to all target instruments.
pub const EVENT_WORKSPACE_PRESET_APPLIED: &str = "workspace:preset-applied";
/// A structured event from the discovery engine.
pub const EVENT_DISCOVERY_EVENT: &str = "discovery:event";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

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
                Error::Usb(_) => "usb",
                Error::UsbTmc(_) => "usbtmc",
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

/// The standard "no instrument connected" error.
fn no_session() -> CommandError {
    CommandError::new(
        "no-session",
        "No instrument connected. Open one from Search.",
    )
}

/// The standard "no such instrument" error.
fn no_instrument(id: impl std::fmt::Display) -> CommandError {
    CommandError::new(
        "no-instrument",
        format!("No instrument '{id}' in the active workspace."),
    )
}

/// The standard "no active workspace" error.
fn no_workspace() -> CommandError {
    CommandError::new("no-workspace", "No active workspace.")
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared application state.
///
/// Cloning is cheap: every field is an `Arc`, so a clone can be moved into a
/// `spawn_blocking` task while the managed copy stays put.
#[derive(Clone)]
pub struct WorkspaceState {
    /// The set of workspaces and the active one.
    manager: Arc<Mutex<WorkspaceManager>>,
    /// Live connections, keyed by instrument id.
    sessions: Arc<Mutex<HashMap<InstrumentId, Connection>>>,
    /// The embedded Python engine, created on first use.
    engine: Arc<Mutex<Option<ScriptEngine>>>,
    /// The cancellation handle of the running script, if any.
    script_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    /// The discovery engine and its shared TTL cache.
    discovery: DiscoveryEngine,
}

impl WorkspaceState {
    /// An in-memory state with no persistence.
    pub fn new() -> Self {
        WorkspaceState {
            manager: Arc::new(Mutex::new(WorkspaceManager::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            engine: Arc::new(Mutex::new(None)),
            script_cancel: Arc::new(Mutex::new(None)),
            discovery: DiscoveryEngine::new(DEFAULT_TTL_SECS),
        }
    }

    /// A state that persists workspaces to `dir` and loads any already there.
    pub fn with_dir(dir: impl Into<std::path::PathBuf>) -> Self {
        let mut manager = WorkspaceManager::with_dir(dir);
        let _ = manager.load_all();
        WorkspaceState {
            manager: Arc::new(Mutex::new(manager)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            engine: Arc::new(Mutex::new(None)),
            script_cancel: Arc::new(Mutex::new(None)),
            discovery: DiscoveryEngine::new(DEFAULT_TTL_SECS),
        }
    }

    /// Ensure an active workspace exists, creating a default one if needed.
    fn ensure_workspace(&self) -> WorkspaceId {
        let mut manager = self.manager.lock().unwrap();
        if let Some(id) = manager.active_id() {
            return id.clone();
        }
        manager.create("Default Workspace")
    }

    /// Record a timeline event on the active workspace.
    fn record_timeline(
        &self,
        instrument_id: Option<InstrumentId>,
        kind: TimelineEventKind,
        message: impl Into<String>,
    ) {
        let mut manager = self.manager.lock().unwrap();
        if let Some(workspace) = manager.active_mut() {
            workspace.timeline.record(TimelineEvent::new(
                now_millis(),
                instrument_id,
                kind,
                message,
            ));
        }
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a transport target from an instrument's configuration.
fn target_for(instrument: &Instrument) -> Target {
    let mut target = Target::from_device(&instrument.device, instrument.config.transport)
        .with_timeout(instrument.config.timeout());
    if let Some(port) = instrument.config.port {
        target = target.with_port(port);
    }
    target.device_name = instrument.device.device_name.clone();
    target
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// An instrument as the frontend needs it, with derived display fields.
#[derive(Debug, Clone, Serialize)]
pub struct InstrumentView {
    pub id: String,
    pub label: String,
    pub address: String,
    pub transport: Transport,
    pub port: Option<u16>,
    pub state: InstrumentState,
    pub idn: Option<String>,
    pub instrument_type: String,
    pub instrument_glyph: String,
    pub last_error: Option<String>,
    pub capabilities: Vec<String>,
}

impl From<&Instrument> for InstrumentView {
    fn from(instrument: &Instrument) -> Self {
        let driver = instrument.idn.as_deref().map(InstrumentDriver::from_idn);
        let (instrument_type, glyph) = match &driver {
            Some(driver) => (
                driver.instrument_type.label().to_string(),
                driver.instrument_type.glyph().to_string(),
            ),
            None => ("instrument".to_string(), "inst".to_string()),
        };
        InstrumentView {
            id: instrument.id.to_string(),
            label: instrument.label(),
            // `display_address` renders a USB instrument as its `usb:...`
            // selector; the raw `address` is `0.0.0.0` for USB and would show
            // the wrong thing in the workspace and topology views.
            address: instrument.device.display_address(),
            transport: instrument.config.transport,
            port: instrument.config.port,
            state: instrument.state,
            idn: instrument.idn.clone(),
            instrument_type,
            instrument_glyph: glyph,
            last_error: instrument.last_error.clone(),
            capabilities: instrument
                .capabilities
                .iter()
                .map(|c| c.to_string())
                .collect(),
        }
    }
}

/// A workspace summary for the workspace list.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub instrument_count: usize,
    pub connected_count: usize,
    pub modified_ms: u64,
    pub active: bool,
}

/// The full active workspace, with the timeline reduced to a count.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceView {
    pub id: String,
    pub name: String,
    pub instruments: Vec<InstrumentView>,
    pub presets: Vec<Preset>,
    pub rules: Vec<Rule>,
    pub triggers: Vec<Trigger>,
    pub timeline_len: usize,
    pub modified_ms: u64,
}

impl WorkspaceView {
    fn from_workspace(workspace: &Workspace) -> Self {
        WorkspaceView {
            id: workspace.id.to_string(),
            name: workspace.name.clone(),
            instruments: workspace
                .instruments
                .iter()
                .map(InstrumentView::from)
                .collect(),
            presets: workspace.presets.clone(),
            rules: workspace.rules.clone(),
            triggers: workspace.triggers.clone(),
            timeline_len: workspace.timeline.len(),
            modified_ms: workspace.modified_ms,
        }
    }
}

/// A payload describing an instrument state change.
#[derive(Debug, Clone, Serialize)]
pub struct InstrumentEvent {
    pub workspace_id: String,
    pub instrument_id: String,
    pub state: InstrumentState,
    pub idn: Option<String>,
    pub message: Option<String>,
    pub timestamp_ms: u64,
}

/// A payload describing a rule or trigger firing.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationEvent {
    pub workspace_id: String,
    pub id: String,
    pub source: String,
    pub target: String,
    pub command: Option<String>,
    pub message: String,
    pub timestamp_ms: u64,
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

/// The application version, shown in the About panel.
#[tauri::command]
pub fn version() -> String {
    omariglol_core::VERSION.to_string()
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Discovery engine
// ---------------------------------------------------------------------------

/// A serde default of `true`.
fn default_true() -> bool {
    true
}

/// The default broadcast/mDNS listen window, in seconds.
fn default_listen_secs() -> u64 {
    3
}

/// The default cache TTL, in seconds.
fn default_ttl() -> u64 {
    DEFAULT_TTL_SECS
}

/// Arguments for [`discovery_scan`].
#[derive(Debug, Deserialize)]
pub struct DiscoveryScanRequest {
    /// Run the VXI-11 broadcast strategy.
    #[serde(default = "default_true")]
    pub use_broadcast: bool,
    /// Run the mDNS/DNS-SD strategy.
    #[serde(default = "default_true")]
    pub use_mdns: bool,
    /// Scan the local subnet with direct TCP probes.
    #[serde(default = "default_true")]
    pub scan_subnet: bool,
    /// Additional hosts to probe (static addresses, other subnets).
    #[serde(default)]
    pub extra_hosts: Vec<String>,
    /// The broadcast/mDNS listen window, in seconds.
    #[serde(default = "default_listen_secs")]
    pub timeout_secs: u64,
    /// The TTL applied to new cache entries, in seconds.
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
    /// Skip hosts whose cache entry is still fresh.
    #[serde(default = "default_true")]
    pub incremental: bool,
}

/// Describe a network configuration in one line, for the timeline.
fn describe_network(config: &NetworkConfig) -> String {
    match &config.primary {
        Some(interface) => {
            let vlan = interface
                .vlan
                .map(|vlan| format!(" (vlan {vlan})"))
                .unwrap_or_default();
            format!(
                "{}/{} on {}{}",
                interface.address, interface.prefix_len, interface.name, vlan
            )
        }
        None => "offline".to_string(),
    }
}

/// Run the multi-strategy discovery engine.
///
/// Emits `discovery:event` events and records structured events on the shared
/// timeline for audit and replay.
#[tauri::command]
pub async fn discovery_scan(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    request: DiscoveryScanRequest,
) -> CommandResult<DiscoveryReport> {
    let state = state.inner().clone();
    let engine = state.discovery.clone();
    let extra_hosts: Vec<IpAddr> = request
        .extra_hosts
        .iter()
        .filter_map(|host| host.parse().ok())
        .collect();
    let options = DiscoveryOptions {
        use_broadcast: request.use_broadcast,
        use_mdns: request.use_mdns,
        scan_subnet: request.scan_subnet,
        extra_hosts,
        ttl_secs: request.ttl_secs,
        incremental: request.incremental,
        listen_timeout: Duration::from_secs(request.timeout_secs),
        ..Default::default()
    };

    tauri::async_runtime::spawn_blocking(move || {
        let state_for_events = state.clone();
        let app_for_events = app.clone();
        let on_event = move |event: DiscoveryEvent| {
            let _ = app_for_events.emit(EVENT_DISCOVERY_EVENT, event.clone());
            let (kind, message) = match &event {
                DiscoveryEvent::Started { strategies } => (
                    TimelineEventKind::Marker,
                    format!("discovery started: {}", strategies.join(", ")),
                ),
                DiscoveryEvent::NetworkDetected { config } => (
                    TimelineEventKind::Marker,
                    format!("network: {}", describe_network(config)),
                ),
                DiscoveryEvent::InstrumentFound { instrument } => {
                    let label = if instrument.model.is_empty() {
                        "instrument"
                    } else {
                        instrument.model.as_str()
                    };
                    (
                        TimelineEventKind::Marker,
                        format!("found {label} at {}", instrument.display_address()),
                    )
                }
                // Progress is too noisy for the timeline; it is emitted as an
                // event only.
                DiscoveryEvent::Progress { .. } => return,
                DiscoveryEvent::Finished { found, elapsed_ms } => (
                    TimelineEventKind::Marker,
                    format!("discovery finished: {found} found in {elapsed_ms:.0} ms"),
                ),
            };
            state_for_events.record_timeline(None, kind, message);
        };
        Ok(engine.run(&options, on_event))
    })
    .await
    .map_err(join_error)?
}

/// Return every cached discovery result, fresh or not.
#[tauri::command]
pub async fn discovery_cache(
    state: State<'_, WorkspaceState>,
) -> CommandResult<Vec<CachedInstrument>> {
    let state = state.inner().clone();
    let cache = state.discovery.cache();
    let guard = cache.lock().unwrap();
    Ok(guard.all().into_iter().cloned().collect())
}

/// Clear the discovery cache, forcing the next scan to probe everything.
#[tauri::command]
pub async fn discovery_invalidate(state: State<'_, WorkspaceState>) -> CommandResult<()> {
    let state = state.inner().clone();
    let cache = state.discovery.cache();
    cache.lock().unwrap().clear();
    Ok(())
}

/// Detect the current network configuration.
#[tauri::command]
pub async fn network_config() -> CommandResult<NetworkConfig> {
    Ok(netconfig::detect())
}

// ---------------------------------------------------------------------------
// Workspace management
// ---------------------------------------------------------------------------

/// Create a new workspace and make it active.
#[tauri::command]
pub async fn workspace_create(
    state: State<'_, WorkspaceState>,
    name: String,
) -> CommandResult<WorkspaceView> {
    let state = state.inner().clone();
    let mut manager = state.manager.lock().unwrap();
    let id = manager.create(name);
    let _ = manager.save(&id);
    let workspace = manager.get(&id).ok_or_else(no_workspace)?;
    Ok(WorkspaceView::from_workspace(workspace))
}

/// List all saved workspaces with summary metadata.
#[tauri::command]
pub async fn workspace_list(
    state: State<'_, WorkspaceState>,
) -> CommandResult<Vec<WorkspaceSummary>> {
    let state = state.inner().clone();
    let manager = state.manager.lock().unwrap();
    let active = manager.active_id().cloned();
    Ok(manager
        .list()
        .iter()
        .map(|workspace| WorkspaceSummary {
            id: workspace.id.to_string(),
            name: workspace.name.clone(),
            instrument_count: workspace.instruments.len(),
            connected_count: workspace.connected_count(),
            modified_ms: workspace.modified_ms,
            active: active.as_ref() == Some(&workspace.id),
        })
        .collect())
}

/// Open a workspace, making it active and returning its contents.
#[tauri::command]
pub async fn workspace_open(
    state: State<'_, WorkspaceState>,
    id: String,
) -> CommandResult<WorkspaceView> {
    let state = state.inner().clone();
    let mut manager = state.manager.lock().unwrap();
    let workspace_id = WorkspaceId(id);
    if !manager.open(&workspace_id) {
        return Err(no_workspace());
    }
    let workspace = manager.get(&workspace_id).ok_or_else(no_workspace)?;
    Ok(WorkspaceView::from_workspace(workspace))
}

/// Persist the active workspace.
#[tauri::command]
pub async fn workspace_save(state: State<'_, WorkspaceState>) -> CommandResult<()> {
    let state = state.inner().clone();
    let manager = state.manager.lock().unwrap();
    manager
        .save_active()
        .map_err(|e| CommandError::new("io", format!("could not save the workspace: {e}")))?;
    Ok(())
}

/// Delete a workspace and all its data.
#[tauri::command]
pub async fn workspace_delete(state: State<'_, WorkspaceState>, id: String) -> CommandResult<()> {
    let state = state.inner().clone();
    let mut manager = state.manager.lock().unwrap();
    if !manager.delete(&WorkspaceId(id)) {
        return Err(no_workspace());
    }
    Ok(())
}

/// Return the active workspace, creating a default one if none exists.
#[tauri::command]
pub async fn workspace_get(state: State<'_, WorkspaceState>) -> CommandResult<WorkspaceView> {
    let state = state.inner().clone();
    state.ensure_workspace();
    let manager = state.manager.lock().unwrap();
    let workspace = manager.active().ok_or_else(no_workspace)?;
    Ok(WorkspaceView::from_workspace(workspace))
}

// ---------------------------------------------------------------------------
// Instrument management
// ---------------------------------------------------------------------------

/// Arguments for [`workspace_add_instrument`].
#[derive(Debug, Deserialize)]
pub struct AddInstrumentRequest {
    pub address: String,
    pub transport: Transport,
    pub port: Option<u16>,
    pub timeout_secs: u64,
}

/// Add a discovered or manual instrument to the active workspace.
#[tauri::command]
pub async fn workspace_add_instrument(
    state: State<'_, WorkspaceState>,
    request: AddInstrumentRequest,
) -> CommandResult<InstrumentView> {
    let state = state.inner().clone();
    let mut device = Device::from_address(&request.address, request.transport)?;
    match request.transport {
        Transport::Vxi11 => device.vxi11_port = request.port,
        Transport::Raw => device.raw_port = request.port,
        Transport::Usb => {}
    }

    state.ensure_workspace();
    let mut manager = state.manager.lock().unwrap();
    let workspace = manager.active_mut().ok_or_else(no_workspace)?;
    let id = workspace.add_instrument(device);
    if let Some(instrument) = workspace.instrument_mut(&id) {
        instrument.config.transport = request.transport;
        instrument.config.port = request.port;
        instrument.config.timeout_secs = request.timeout_secs;
    }
    let instrument = workspace
        .instrument(&id)
        .ok_or_else(|| no_instrument(&id))?;
    Ok(InstrumentView::from(instrument))
}

/// Remove an instrument from the active workspace.
#[tauri::command]
pub async fn workspace_remove_instrument(
    state: State<'_, WorkspaceState>,
    instrument_id: String,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let id = InstrumentId(instrument_id);
    // Close any live connection first.
    if let Some(mut connection) = state.sessions.lock().unwrap().remove(&id) {
        connection.close();
    }
    let mut manager = state.manager.lock().unwrap();
    let workspace = manager.active_mut().ok_or_else(no_workspace)?;
    if !workspace.remove_instrument(&id) {
        return Err(no_instrument(&id));
    }
    Ok(())
}

/// List all instruments in the active workspace with their connection status.
#[tauri::command]
pub async fn instrument_list(
    state: State<'_, WorkspaceState>,
) -> CommandResult<Vec<InstrumentView>> {
    let state = state.inner().clone();
    state.ensure_workspace();
    let manager = state.manager.lock().unwrap();
    let workspace = manager.active().ok_or_else(no_workspace)?;
    Ok(workspace
        .instruments
        .iter()
        .map(InstrumentView::from)
        .collect())
}

/// Open a connection to an instrument in the active workspace.
#[tauri::command]
pub async fn instrument_connect(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    instrument_id: String,
) -> CommandResult<InstrumentView> {
    let state = state.inner().clone();
    let id = InstrumentId(instrument_id);

    // 1. Read the target and workspace id, then release the manager lock.
    let (target, workspace_id) = {
        let manager = state.manager.lock().unwrap();
        let workspace = manager.active().ok_or_else(no_workspace)?;
        let instrument = workspace
            .instrument(&id)
            .ok_or_else(|| no_instrument(&id))?;
        (target_for(instrument), workspace.id.to_string())
    };

    // 2. Do the network work under the session lock only.
    let id_for_task = id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut connection = Connection::new(id_for_task, target);
        connection.open()?;
        let idn = connection.identify()?;
        Ok::<_, Error>((connection, idn))
    })
    .await
    .map_err(join_error)?;

    match result {
        Ok((connection, idn)) => {
            state
                .sessions
                .lock()
                .unwrap()
                .insert(id.clone(), connection);
            let now = now_millis();
            let mut manager = state.manager.lock().unwrap();
            if let Some(workspace) = manager.active_mut() {
                if let Some(instrument) = workspace.instrument_mut(&id) {
                    instrument.mark_connected(idn.clone(), now);
                    instrument.capabilities = InstrumentDriver::from_idn(&idn)
                        .commands
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                }
                workspace.timeline.record(TimelineEvent::new(
                    now,
                    Some(id.clone()),
                    TimelineEventKind::Command,
                    format!("connected: {idn}"),
                ));
            }
            let _ = app.emit(
                EVENT_WORKSPACE_INSTRUMENT_CONNECTED,
                InstrumentEvent {
                    workspace_id,
                    instrument_id: id.to_string(),
                    state: InstrumentState::Connected,
                    idn: Some(idn),
                    message: None,
                    timestamp_ms: now,
                },
            );
            let manager = state.manager.lock().unwrap();
            let workspace = manager.active().ok_or_else(no_workspace)?;
            let instrument = workspace
                .instrument(&id)
                .ok_or_else(|| no_instrument(&id))?;
            Ok(InstrumentView::from(instrument))
        }
        Err(err) => {
            let now = now_millis();
            let message = err.to_string();
            let mut manager = state.manager.lock().unwrap();
            if let Some(workspace) = manager.active_mut() {
                if let Some(instrument) = workspace.instrument_mut(&id) {
                    instrument.mark_error(message.clone(), now);
                }
                workspace.timeline.record(TimelineEvent::new(
                    now,
                    Some(id.clone()),
                    TimelineEventKind::Error,
                    message.clone(),
                ));
            }
            let _ = app.emit(
                EVENT_WORKSPACE_INSTRUMENT_ERROR,
                InstrumentEvent {
                    workspace_id,
                    instrument_id: id.to_string(),
                    state: InstrumentState::Error,
                    idn: None,
                    message: Some(message.clone()),
                    timestamp_ms: now,
                },
            );
            Err(CommandError::from(err))
        }
    }
}

/// Close the connection to an instrument, keeping it in the workspace.
#[tauri::command]
pub async fn instrument_disconnect(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    instrument_id: String,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let id = InstrumentId(instrument_id);
    if let Some(mut connection) = state.sessions.lock().unwrap().remove(&id) {
        connection.close();
    }
    let now = now_millis();
    let workspace_id = {
        let mut manager = state.manager.lock().unwrap();
        let workspace = manager.active_mut().ok_or_else(no_workspace)?;
        if let Some(instrument) = workspace.instrument_mut(&id) {
            instrument.mark_disconnected();
        }
        workspace.timeline.record(TimelineEvent::new(
            now,
            Some(id.clone()),
            TimelineEventKind::Command,
            "disconnected",
        ));
        workspace.id.to_string()
    };
    let _ = app.emit(
        EVENT_WORKSPACE_INSTRUMENT_DISCONNECTED,
        InstrumentEvent {
            workspace_id,
            instrument_id: id.to_string(),
            state: InstrumentState::Disconnected,
            idn: None,
            message: None,
            timestamp_ms: now,
        },
    );
    Ok(())
}

/// Send a SCPI query to a specific instrument.
#[tauri::command]
pub async fn instrument_query(
    state: State<'_, WorkspaceState>,
    instrument_id: String,
    command: String,
) -> CommandResult<String> {
    let state = state.inner().clone();
    let id = InstrumentId(instrument_id);
    let state_for_task = state.clone();
    let id_for_task = id.clone();
    let command_for_task = command.clone();
    let response = tauri::async_runtime::spawn_blocking(move || {
        let mut sessions = state_for_task.sessions.lock().unwrap();
        let connection = sessions.get_mut(&id_for_task).ok_or_else(no_session)?;
        let response = connection.query(&command_for_task)?;
        connection.record_success();
        Ok::<_, CommandError>(response)
    })
    .await
    .map_err(join_error)??;

    state.record_timeline(
        Some(id),
        TimelineEventKind::Query,
        format!("{command} -> {response}"),
    );
    Ok(response)
}

/// Send a SCPI command (no response expected) to a specific instrument.
#[tauri::command]
pub async fn instrument_write(
    state: State<'_, WorkspaceState>,
    instrument_id: String,
    command: String,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let id = InstrumentId(instrument_id);
    let command_for_timeline = command.clone();
    let state_for_task = state.clone();
    let id_for_task = id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut sessions = state_for_task.sessions.lock().unwrap();
        let connection = sessions.get_mut(&id_for_task).ok_or_else(no_session)?;
        connection.send(&command)?;
        connection.record_success();
        Ok::<_, CommandError>(())
    })
    .await
    .map_err(join_error)??;

    state.record_timeline(Some(id), TimelineEventKind::Command, command_for_timeline);
    Ok(())
}

/// Return the driver description for an instrument.
///
/// The driver is derived from the instrument's `*IDN?`; for a known family
/// (currently the Rigol DS1000Z) it carries the full operation surface, so the
/// UI can render every control the instrument supports.
#[tauri::command]
pub async fn instrument_driver(
    state: State<'_, WorkspaceState>,
    instrument_id: String,
) -> CommandResult<InstrumentDriver> {
    let state = state.inner().clone();
    let id = InstrumentId(instrument_id);
    let manager = state.manager.lock().unwrap();
    let workspace = manager.active().ok_or_else(no_workspace)?;
    let instrument = workspace
        .instrument(&id)
        .ok_or_else(|| no_instrument(&id))?;
    let idn = instrument.idn.clone().unwrap_or_default();
    Ok(InstrumentDriver::from_idn(&idn))
}

/// Run a named driver command against an instrument.
///
/// `args` is appended to the command's SCPI mnemonic, so a setter such as
/// `channel1_scale` becomes `:CHAN1:SCAL <args>`. Queries ignore `args` and
/// return the response; writes return `None`.
#[tauri::command]
pub async fn instrument_run_command(
    state: State<'_, WorkspaceState>,
    instrument_id: String,
    name: String,
    args: Option<String>,
) -> CommandResult<Option<String>> {
    let inner = state.inner().clone();
    let id = InstrumentId(instrument_id);

    // Resolve the SCPI command from the instrument's driver.
    let (scpi, is_query) = {
        let manager = inner.manager.lock().unwrap();
        let workspace = manager.active().ok_or_else(no_workspace)?;
        let instrument = workspace
            .instrument(&id)
            .ok_or_else(|| no_instrument(&id))?;
        let idn = instrument.idn.clone().unwrap_or_default();
        let driver = InstrumentDriver::from_idn(&idn);
        let command = driver
            .command(&name)
            .ok_or_else(|| CommandError::new("no-command", format!("No command '{name}'.")))?;
        (command.scpi.clone(), command.query)
    };

    let full = match args {
        Some(args) if !args.trim().is_empty() => format!("{scpi} {}", args.trim()),
        _ => scpi,
    };

    if is_query {
        let response = instrument_query(state, id.to_string(), full).await?;
        Ok(Some(response))
    } else {
        instrument_write(state, id.to_string(), full).await?;
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Batch and routing
// ---------------------------------------------------------------------------

/// Arguments for the batch commands: a map from instrument id to command.
#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub commands: HashMap<String, String>,
}

/// Send commands to multiple instruments in parallel.
#[tauri::command]
pub async fn instrument_batch_write(
    state: State<'_, WorkspaceState>,
    request: BatchRequest,
) -> CommandResult<HashMap<String, Option<String>>> {
    let state = state.inner().clone();
    let mut results = HashMap::new();
    for (instrument_id, command) in request.commands {
        let id = InstrumentId(instrument_id.clone());
        let state_clone = state.clone();
        let command_clone = command.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            let mut sessions = state_clone.sessions.lock().unwrap();
            let connection = sessions.get_mut(&id).ok_or_else(no_session)?;
            connection.send(&command_clone)?;
            connection.record_success();
            Ok::<_, CommandError>(())
        })
        .await
        .map_err(join_error)?;
        match outcome {
            Ok(()) => {
                state.record_timeline(
                    Some(InstrumentId(instrument_id.clone())),
                    TimelineEventKind::Command,
                    command,
                );
                results.insert(instrument_id, None);
            }
            Err(err) => {
                results.insert(instrument_id, Some(err.message));
            }
        }
    }
    Ok(results)
}

/// Query multiple instruments in parallel and return a map of results.
#[tauri::command]
pub async fn instrument_batch_query(
    state: State<'_, WorkspaceState>,
    request: BatchRequest,
) -> CommandResult<HashMap<String, Option<String>>> {
    let state = state.inner().clone();
    let mut results = HashMap::new();
    for (instrument_id, command) in request.commands {
        let id = InstrumentId(instrument_id.clone());
        let state_clone = state.clone();
        let command_clone = command.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            let mut sessions = state_clone.sessions.lock().unwrap();
            let connection = sessions.get_mut(&id).ok_or_else(no_session)?;
            let response = connection.query(&command_clone)?;
            connection.record_success();
            Ok::<_, CommandError>(response)
        })
        .await
        .map_err(join_error)?;
        match outcome {
            Ok(response) => {
                state.record_timeline(
                    Some(InstrumentId(instrument_id.clone())),
                    TimelineEventKind::Query,
                    format!("{command} -> {response}"),
                );
                results.insert(instrument_id, Some(response));
            }
            Err(err) => {
                results.insert(instrument_id, Some(err.message));
            }
        }
    }
    Ok(results)
}

/// Evaluate a conditional routing rule and execute its action if the condition
/// is met.
#[tauri::command]
pub async fn rule_evaluate(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    rule_id: String,
) -> CommandResult<Option<String>> {
    let state = state.inner().clone();

    // Read the rule and workspace id.
    let (rule, workspace_id) = {
        let manager = state.manager.lock().unwrap();
        let workspace = manager.active().ok_or_else(no_workspace)?;
        let rule = workspace
            .rules
            .iter()
            .find(|r| r.id == rule_id)
            .cloned()
            .ok_or_else(|| CommandError::new("no-rule", format!("No rule '{rule_id}'.")))?;
        (rule, workspace.id.to_string())
    };

    // Query the source instrument.
    let source_id = rule.source.clone();
    let query = rule.query.clone();
    let state_for_query = state.clone();
    let reading = tauri::async_runtime::spawn_blocking(move || {
        let mut sessions = state_for_query.sessions.lock().unwrap();
        let connection = sessions.get_mut(&source_id).ok_or_else(no_session)?;
        let response = connection.query(&query)?;
        let value = response
            .trim()
            .parse::<f64>()
            .map_err(|_| CommandError::new("malformed", format!("not a number: {response}")))?;
        Ok::<_, CommandError>(value)
    })
    .await
    .map_err(join_error)??;

    let decision = rule.evaluate(reading);
    let now = now_millis();

    // Send the action, if any.
    if let Some(command) = &decision.command {
        let target_id = decision.target.clone();
        let command_clone = command.clone();
        let state_clone = state.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let mut sessions = state_clone.sessions.lock().unwrap();
            if let Some(connection) = sessions.get_mut(&target_id) {
                let _ = connection.send(&command_clone);
            }
        })
        .await;
    }

    state.record_timeline(
        Some(rule.source.clone()),
        TimelineEventKind::Rule,
        format!(
            "{}: {} {} -> {}",
            rule.name,
            rule.condition_text(),
            reading,
            decision.command.as_deref().unwrap_or("(no action)")
        ),
    );

    let _ = app.emit(
        EVENT_WORKSPACE_RULE_FIRED,
        AutomationEvent {
            workspace_id,
            id: rule.id.clone(),
            source: rule.source.to_string(),
            target: rule.target.to_string(),
            command: decision.command.clone(),
            message: format!("{} = {reading}", rule.condition_text()),
            timestamp_ms: now,
        },
    );

    Ok(decision.command)
}

/// Arm a trigger so it fires when its condition is met.
#[tauri::command]
pub async fn trigger_arm(
    state: State<'_, WorkspaceState>,
    trigger_id: String,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let mut manager = state.manager.lock().unwrap();
    let workspace = manager.active_mut().ok_or_else(no_workspace)?;
    let trigger = workspace
        .triggers
        .iter_mut()
        .find(|t| t.id == trigger_id)
        .ok_or_else(|| CommandError::new("no-trigger", format!("No trigger '{trigger_id}'.")))?;
    trigger.arm();
    Ok(())
}

/// Disarm a trigger.
#[tauri::command]
pub async fn trigger_disarm(
    state: State<'_, WorkspaceState>,
    trigger_id: String,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let mut manager = state.manager.lock().unwrap();
    let workspace = manager.active_mut().ok_or_else(no_workspace)?;
    let trigger = workspace
        .triggers
        .iter_mut()
        .find(|t| t.id == trigger_id)
        .ok_or_else(|| CommandError::new("no-trigger", format!("No trigger '{trigger_id}'.")))?;
    trigger.disarm();
    Ok(())
}

/// Add a trigger edge, validating that the dependency graph stays acyclic.
#[tauri::command]
pub async fn trigger_add(
    state: State<'_, WorkspaceState>,
    source: String,
    target: String,
    condition: String,
    action: String,
) -> CommandResult<Trigger> {
    let state = state.inner().clone();
    let mut manager = state.manager.lock().unwrap();
    let workspace = manager.active_mut().ok_or_else(no_workspace)?;
    let id = format!("trg-{}", now_millis());
    let trigger = Trigger::new(
        id,
        InstrumentId(source),
        InstrumentId(target),
        condition,
        action,
    );
    workspace
        .add_trigger(trigger.clone())
        .map_err(|e| CommandError::new("trigger", e.to_string()))?;
    Ok(trigger)
}

// ---------------------------------------------------------------------------
// Presets and timeline
// ---------------------------------------------------------------------------

/// Apply a preset to all instruments in the active workspace.
#[tauri::command]
pub async fn preset_apply(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    preset_id: String,
) -> CommandResult<HashMap<String, Option<String>>> {
    let state = state.inner().clone();

    let (plan, workspace_id) = {
        let manager = state.manager.lock().unwrap();
        let workspace = manager.active().ok_or_else(no_workspace)?;
        let preset = workspace
            .presets
            .iter()
            .find(|p| p.id == preset_id)
            .ok_or_else(|| CommandError::new("no-preset", format!("No preset '{preset_id}'.")))?;
        let plan = preset.plan(&workspace.instrument_ids());
        (plan, workspace.id.to_string())
    };

    let mut results: HashMap<String, Option<String>> = HashMap::new();
    for step in &plan.steps {
        let id = step.instrument.clone();
        let command = step.command.clone();
        let state_clone = state.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            let mut sessions = state_clone.sessions.lock().unwrap();
            let connection = sessions.get_mut(&id).ok_or_else(no_session)?;
            connection.send(&command)?;
            connection.record_success();
            Ok::<_, CommandError>(())
        })
        .await
        .map_err(join_error)?;
        match outcome {
            Ok(()) => {
                results.insert(step.instrument.to_string(), None);
            }
            Err(err) => {
                results.insert(step.instrument.to_string(), Some(err.message));
            }
        }
    }

    state.record_timeline(
        None,
        TimelineEventKind::Preset,
        format!(
            "applied preset '{}' ({} steps)",
            plan.preset_name,
            plan.len()
        ),
    );

    let _ = app.emit(
        EVENT_WORKSPACE_PRESET_APPLIED,
        AutomationEvent {
            workspace_id,
            id: plan.preset_id.clone(),
            source: String::new(),
            target: String::new(),
            command: None,
            message: format!("applied '{}'", plan.preset_name),
            timestamp_ms: now_millis(),
        },
    );

    Ok(results)
}

/// Retrieve timeline events for a time range.
#[tauri::command]
pub async fn timeline_get(
    state: State<'_, WorkspaceState>,
    start_ms: u64,
    end_ms: u64,
) -> CommandResult<Vec<TimelineEvent>> {
    let state = state.inner().clone();
    let manager = state.manager.lock().unwrap();
    let workspace = manager.active().ok_or_else(no_workspace)?;
    Ok(workspace
        .timeline
        .range(start_ms, end_ms)
        .into_iter()
        .cloned()
        .collect())
}

/// Jump to a point in the timeline and return the most recent event there.
#[tauri::command]
pub async fn timeline_scrub(
    state: State<'_, WorkspaceState>,
    timestamp_ms: u64,
) -> CommandResult<Option<TimelineEvent>> {
    let state = state.inner().clone();
    let manager = state.manager.lock().unwrap();
    let workspace = manager.active().ok_or_else(no_workspace)?;
    Ok(workspace.timeline.at(timestamp_ms).cloned())
}

// ---------------------------------------------------------------------------
// Legacy single-instrument commands (kept for compatibility)
// ---------------------------------------------------------------------------

/// Arguments for [`connect`].
#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub address: String,
    pub transport: Transport,
    pub port: Option<u16>,
    pub timeout_secs: u64,
}

/// Add an instrument to the active workspace and connect it.
///
/// This is the single-instrument entry point retained for compatibility; the
/// workspace-scoped [`workspace_add_instrument`] + [`instrument_connect`] pair
/// is the multi-instrument evolution.
#[tauri::command]
pub async fn connect(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    request: ConnectRequest,
) -> CommandResult<String> {
    let view = workspace_add_instrument(
        state.clone(),
        AddInstrumentRequest {
            address: request.address,
            transport: request.transport,
            port: request.port,
            timeout_secs: request.timeout_secs,
        },
    )
    .await?;
    let connected = instrument_connect(app, state, view.id).await?;
    Ok(connected.idn.unwrap_or_default())
}

/// Close every connection in the active workspace.
#[tauri::command]
pub async fn disconnect(state: State<'_, WorkspaceState>) -> CommandResult<()> {
    let state = state.inner().clone();
    let ids: Vec<InstrumentId> = {
        let manager = state.manager.lock().unwrap();
        manager
            .active()
            .map(|workspace| workspace.instrument_ids())
            .unwrap_or_default()
    };
    let mut sessions = state.sessions.lock().unwrap();
    for id in ids {
        if let Some(mut connection) = sessions.remove(&id) {
            connection.close();
        }
    }
    Ok(())
}

/// Send a SCPI query to the first connected instrument.
#[tauri::command]
pub async fn scpi_query(
    state: State<'_, WorkspaceState>,
    command: String,
) -> CommandResult<String> {
    let id = first_connected(&state)?;
    instrument_query(state, id.to_string(), command).await
}

/// Send a SCPI command to the first connected instrument.
#[tauri::command]
pub async fn scpi_write(state: State<'_, WorkspaceState>, command: String) -> CommandResult<()> {
    let id = first_connected(&state)?;
    instrument_write(state, id.to_string(), command).await
}

/// The id of the first connected instrument in the active workspace.
fn first_connected(state: &WorkspaceState) -> CommandResult<InstrumentId> {
    let manager = state.manager.lock().unwrap();
    let workspace = manager.active().ok_or_else(no_workspace)?;
    workspace
        .instruments
        .iter()
        .find(|instrument| instrument.state.is_connected())
        .map(|instrument| instrument.id.clone())
        .ok_or_else(no_session)
}

/// Arguments for [`capture_screenshot`].
#[derive(Debug, Deserialize)]
pub struct ScreenshotRequest {
    /// Overrides the profile's capture query.
    pub command: Option<String>,
    /// The instrument to capture, or the first connected one.
    #[serde(default)]
    pub instrument_id: Option<String>,
}

/// A screenshot ready for the frontend: the image is base64-encoded.
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

/// Capture an instrument's screen.
#[tauri::command]
pub async fn capture_screenshot(
    state: State<'_, WorkspaceState>,
    request: ScreenshotRequest,
) -> CommandResult<ScreenshotResponse> {
    let state = state.inner().clone();
    let id = match request.instrument_id {
        Some(id) => InstrumentId(id),
        None => first_connected(&state)?,
    };

    let (device, transport_kind, port, timeout) = {
        let manager = state.manager.lock().unwrap();
        let workspace = manager.active().ok_or_else(no_workspace)?;
        let instrument = workspace
            .instrument(&id)
            .ok_or_else(|| no_instrument(&id))?;
        (
            instrument.device.clone(),
            instrument.config.transport,
            instrument.config.port,
            instrument.config.timeout(),
        )
    };

    let command = request.command;
    tauri::async_runtime::spawn_blocking(move || {
        let shot = screenshot::capture_from_device(
            &device,
            transport_kind,
            port,
            timeout,
            command.as_deref(),
        )?;
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
    /// The instrument to benchmark, or the first connected one.
    #[serde(default)]
    pub instrument_id: Option<String>,
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
    state: State<'_, WorkspaceState>,
    request: BenchmarkRequest,
) -> CommandResult<BenchmarkReport> {
    let state = state.inner().clone();
    let id = match request.instrument_id {
        Some(id) => InstrumentId(id),
        None => first_connected(&state)?,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut sessions = state.sessions.lock().unwrap();
        let connection = sessions.get_mut(&id).ok_or_else(no_session)?;
        let options = BenchmarkOptions {
            command: request.command,
            iterations: request.iterations,
            ..Default::default()
        };
        let report = benchmark::run(
            connection.session_mut()?.transport_mut(),
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
    state: State<'_, WorkspaceState>,
    request: ScriptRequest,
) -> CommandResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
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
pub async fn cancel_script(state: State<'_, WorkspaceState>) -> CommandResult<()> {
    if let Some(flag) = state.script_cancel.lock().unwrap().clone() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Build a target from a connect request. Kept separate so the conversion is
/// testable independently of the IPC layer.
pub fn target_from_request(request: &ConnectRequest) -> omariglol_core::Result<Target> {
    let device = Device::from_address(&request.address, request.transport)?;
    let mut target = Target::from_device(&device, request.transport)
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
    }

    #[test]
    fn test_workspace_state_ensure_workspace() {
        let state = WorkspaceState::new();
        let id = state.ensure_workspace();
        // Calling again returns the same workspace.
        assert_eq!(state.ensure_workspace(), id);
    }

    #[test]
    fn test_instrument_view_from_instrument() {
        let mut instrument =
            Instrument::from_device(Device::manual("192.168.1.50".parse::<IpAddr>().unwrap()));
        instrument.mark_connected("RIGOL TECHNOLOGIES,DS1054Z,SN,1.0".to_string(), 1);
        let view = InstrumentView::from(&instrument);
        assert_eq!(view.instrument_type, "oscilloscope");
        assert_eq!(view.instrument_glyph, "scope");
        assert_eq!(view.state, InstrumentState::Connected);
        assert_eq!(view.address, "192.168.1.50");
    }

    #[test]
    fn test_instrument_view_usb_address_is_selector_not_unspecified_ip() {
        // A USB instrument's `device.address` is `0.0.0.0`; the view must expose
        // the `usb:...` selector instead, matching the discovery panel.
        let selector =
            omariglol_core::device::UsbSelector::new(0x1AB1, 0x04CE).with_serial("DS1ZA1");
        let instrument = Instrument::from_device(Device::usb(selector));
        let view = InstrumentView::from(&instrument);
        assert_eq!(view.address, "usb:0x1AB1:0x04CE:DS1ZA1");
        assert!(!view.address.contains("0.0.0.0"));
        assert_eq!(view.transport, Transport::Usb);
    }

    #[test]
    fn test_target_for_uses_config() {
        let mut instrument =
            Instrument::from_device(Device::manual("192.168.1.50".parse::<IpAddr>().unwrap()));
        instrument.config.transport = Transport::Raw;
        instrument.config.port = Some(5555);
        instrument.config.timeout_secs = 9;
        let target = target_for(&instrument);
        assert_eq!(target.transport, Transport::Raw);
        assert_eq!(target.port, Some(5555));
        assert_eq!(target.timeout, Duration::from_secs(9));
    }
}
