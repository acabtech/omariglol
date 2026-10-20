//! The `Workspace` Python API (concepts.md §5.2).
//!
//! This is the multi-instrument evolution of the `connect()` shim: scripts open
//! a workspace, address instruments by name, and run automation sequences that
//! are recorded on the shared timeline.
//!
//! ```python
//! from omariglol import Workspace
//!
//! ws = Workspace.open("my-workspace")
//! scope = ws.instruments["Rigol DS1054Z"]
//! freq = float(scope.query(":MEAS:FREQ?"))
//!
//! results = ws.batch_query({scope: ":MEAS:FREQ?"})
//! ws.apply_preset("Rack Power-Up")
//!
//! with ws.run("frequency-sweep") as run:
//!     scope.write(":RUN")
//!     run.wait(scope, "*OPC?")
//!     run.record(freq=freq)
//! ```
//!
//! The workspace is loaded from (and saved to) a directory of JSON files, the
//! same format the desktop app persists. Connections are opened lazily and
//! owned by the workspace, so a script never manages sockets by hand.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyKeyError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};

use omariglol_core::device::{Target, Transport};
use omariglol_core::discovery_engine::{CachedInstrument, DiscoveryEngine, DiscoveryOptions};
use omariglol_core::instrument::InstrumentId;
use omariglol_core::preset::Preset;
use omariglol_core::rule::{Comparison, EvaluationMode, Rule};
use omariglol_core::timeline::{TimelineEvent, TimelineEventKind};
use omariglol_core::transport::{parse_address, Session};
use omariglol_core::trigger::Trigger;
use omariglol_core::workspace::{now_millis, Workspace as CoreWorkspace};

use crate::context;

/// The shared, mutable state behind a workspace and its instruments.
struct WorkspaceData {
    inner: CoreWorkspace,
    sessions: HashMap<InstrumentId, Session>,
    dir: Option<PathBuf>,
    /// The discovery engine and its TTL cache, owned by the workspace.
    discovery: DiscoveryEngine,
    /// User-assigned names for instruments, so a script can bind a friendly
    /// reference to a discovered endpoint.
    aliases: HashMap<String, InstrumentId>,
}

impl WorkspaceData {
    /// The id of an instrument, resolved from an alias, id or label.
    fn resolve_id(&self, key: &str) -> Option<InstrumentId> {
        if let Some(id) = self.aliases.get(key) {
            return Some(id.clone());
        }
        if let Some(instrument) = self.inner.instrument(&InstrumentId::new(key)) {
            return Some(instrument.id.clone());
        }
        self.inner
            .instrument_by_label(key)
            .map(|instrument| instrument.id.clone())
    }

    /// Record a timeline event.
    fn record(&mut self, instrument: Option<InstrumentId>, kind: TimelineEventKind, message: String) {
        self.inner.timeline.record(TimelineEvent::new(
            now_millis(),
            instrument,
            kind,
            message,
        ));
    }
}

/// Translate a core error into a Python exception.
fn to_pyerr(err: omariglol_core::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

/// The default directory workspaces are stored in.
fn default_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("omariglol")
            .join("workspaces");
    }
    PathBuf::from("omariglol-workspaces")
}

/// Load every workspace JSON in `dir` and return the one matching `name_or_id`.
fn load_workspace(dir: &PathBuf, name_or_id: &str) -> Option<CoreWorkspace> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(workspace) = serde_json::from_str::<CoreWorkspace>(&text) else {
            continue;
        };
        if workspace.name == name_or_id || workspace.id.as_str() == name_or_id {
            return Some(workspace);
        }
    }
    None
}

/// A live connection to one instrument, as seen from Python.
#[pyclass]
pub struct WorkspaceInstrument {
    data: Arc<Mutex<WorkspaceData>>,
    id: InstrumentId,
    name: String,
}

#[pymethods]
impl WorkspaceInstrument {
    /// The instrument's display name.
    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    /// The instrument's stable id.
    #[getter]
    fn id(&self) -> String {
        self.id.to_string()
    }

    /// The instrument's `*IDN?` string, when known.
    #[getter]
    fn idn(&self) -> Option<String> {
        let data = self.data.lock().unwrap();
        data.inner
            .instrument(&self.id)
            .and_then(|instrument| instrument.idn.clone())
    }

    /// Write a command, adding a terminator if the caller omitted one.
    fn write(&self, command: &str) -> PyResult<()> {
        context::checkpoint().map_err(crate::module::script_err_to_py)?;
        let mut data = self.data.lock().unwrap();
        let session = data
            .sessions
            .get_mut(&self.id)
            .ok_or_else(|| PyRuntimeError::new_err(format!("{} is not connected", self.name)))?;
        session.send(command).map_err(to_pyerr)?;
        data.record(
            Some(self.id.clone()),
            TimelineEventKind::Command,
            command.to_string(),
        );
        Ok(())
    }

    /// Send a query and return the response as a trimmed string.
    fn query(&self, command: &str) -> PyResult<String> {
        context::checkpoint().map_err(crate::module::script_err_to_py)?;
        let mut data = self.data.lock().unwrap();
        let session = data
            .sessions
            .get_mut(&self.id)
            .ok_or_else(|| PyRuntimeError::new_err(format!("{} is not connected", self.name)))?;
        let response = session.query(command).map_err(to_pyerr)?;
        data.record(
            Some(self.id.clone()),
            TimelineEventKind::Query,
            format!("{command} -> {response}"),
        );
        Ok(response)
    }

    /// Send a query and return the raw response bytes.
    fn query_raw<'py>(&self, py: Python<'py>, command: &str) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        context::checkpoint().map_err(crate::module::script_err_to_py)?;
        let mut data = self.data.lock().unwrap();
        let session = data
            .sessions
            .get_mut(&self.id)
            .ok_or_else(|| PyRuntimeError::new_err(format!("{} is not connected", self.name)))?;
        let bytes = session.query_raw(command).map_err(to_pyerr)?;
        Ok(pyo3::types::PyBytes::new(py, &bytes))
    }

    /// Identify the instrument with `*IDN?`.
    fn identify(&self) -> PyResult<String> {
        self.query("*IDN?")
    }

    /// The named operations the instrument's driver exposes.
    fn commands(&self) -> Vec<String> {
        let data = self.data.lock().unwrap();
        let idn = data
            .inner
            .instrument(&self.id)
            .and_then(|instrument| instrument.idn.clone())
            .unwrap_or_default();
        omariglol_core::driver::InstrumentDriver::from_idn(&idn)
            .commands
            .iter()
            .map(|command| command.name.clone())
            .collect()
    }

    /// Run a named driver command, appending `args` to its SCPI mnemonic.
    #[pyo3(signature = (name, args=None))]
    fn run_command(&self, name: &str, args: Option<&str>) -> PyResult<Option<String>> {
        let (scpi, is_query) = {
            let data = self.data.lock().unwrap();
            let idn = data
                .inner
                .instrument(&self.id)
                .and_then(|instrument| instrument.idn.clone())
                .unwrap_or_default();
            let driver = omariglol_core::driver::InstrumentDriver::from_idn(&idn);
            let command = driver
                .command(name)
                .ok_or_else(|| PyValueError::new_err(format!("no command '{name}'")))?;
            (command.scpi.clone(), command.query)
        };
        let full = match args {
            Some(args) if !args.trim().is_empty() => format!("{scpi} {}", args.trim()),
            _ => scpi,
        };
        if is_query {
            Ok(Some(self.query(&full)?))
        } else {
            self.write(&full)?;
            Ok(None)
        }
    }

    fn __repr__(&self) -> String {
        format!("<omariglol.WorkspaceInstrument {}>", self.name)
    }
}

/// A recorded automation run, used as a context manager.
#[pyclass]
pub struct WorkspaceRun {
    data: Arc<Mutex<WorkspaceData>>,
    name: String,
    last: Option<InstrumentId>,
}

#[pymethods]
impl WorkspaceRun {
    /// The run's name.
    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    /// Block until a query returns `1` (typically `*OPC?`).
    ///
    /// `instrument` may be a [`WorkspaceInstrument`] or a name; when omitted the
    /// last instrument used in this run is targeted.
    #[pyo3(signature = (query, instrument=None))]
    fn wait(&mut self, query: &str, instrument: Option<Bound<'_, PyAny>>) -> PyResult<String> {
        let id = match instrument {
            Some(obj) => resolve_instrument(&self.data, &obj)?,
            None => self
                .last
                .clone()
                .ok_or_else(|| PyRuntimeError::new_err("no instrument to wait on"))?,
        };
        self.last = Some(id.clone());
        let mut data = self.data.lock().unwrap();
        let session = data
            .sessions
            .get_mut(&id)
            .ok_or_else(|| PyRuntimeError::new_err("instrument is not connected"))?;
        let response = session.query(query).map_err(to_pyerr)?;
        data.record(
            Some(id),
            TimelineEventKind::Query,
            format!("wait {query} -> {response}"),
        );
        Ok(response)
    }

    /// Record arbitrary key-value data at this point in the run.
    #[pyo3(signature = (**kwargs))]
    fn record(&mut self, kwargs: Option<Bound<'_, PyDict>>) -> PyResult<()> {
        let mut data = self.data.lock().unwrap();
        let payload = match kwargs {
            Some(dict) => {
                let mut map = serde_json::Map::new();
                for (key, value) in dict.iter() {
                    let key: String = key.extract()?;
                    let value: serde_json::Value = python_to_json(&value)?;
                    map.insert(key, value);
                }
                serde_json::Value::Object(map)
            }
            None => serde_json::Value::Null,
        };
        let event = TimelineEvent::new(
            now_millis(),
            self.last.clone(),
            TimelineEventKind::Script,
            format!("record @ {}", self.name),
        )
        .with_data(payload);
        data.inner.timeline.record(event);
        Ok(())
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_value=None, _traceback=None))]
    fn __exit__(
        &mut self,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_value: Option<Bound<'_, PyAny>>,
        _traceback: Option<Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        let mut data = self.data.lock().unwrap();
        data.record(
            self.last.clone(),
            TimelineEventKind::Script,
            format!("run '{}' finished", self.name),
        );
        Ok(false)
    }
}

/// A workspace: a named collection of instruments and automation artifacts.
#[pyclass]
pub struct Workspace {
    data: Arc<Mutex<WorkspaceData>>,
}

impl Workspace {
    /// Build a Python workspace from a core workspace and a directory.
    fn from_core(inner: CoreWorkspace, dir: Option<PathBuf>) -> Self {
        Workspace {
            data: Arc::new(Mutex::new(WorkspaceData {
                inner,
                sessions: HashMap::new(),
                dir,
                discovery: DiscoveryEngine::default(),
                aliases: HashMap::new(),
            })),
        }
    }
}

#[pymethods]
impl Workspace {
    /// Open a saved workspace by name or id.
    #[pyo3(signature = (name, dir=None))]
    #[staticmethod]
    fn open(name: &str, dir: Option<PathBuf>) -> PyResult<Self> {
        let dir = dir.unwrap_or_else(default_dir);
        let workspace = load_workspace(&dir, name).ok_or_else(|| {
            PyKeyError::new_err(format!(
                "no workspace '{name}' in {}",
                dir.display()
            ))
        })?;
        Ok(Workspace::from_core(workspace, Some(dir)))
    }

    /// Create a new, empty workspace.
    #[pyo3(signature = (name, dir=None))]
    #[staticmethod]
    fn create(name: &str, dir: Option<PathBuf>) -> PyResult<Self> {
        let dir = dir.unwrap_or_else(default_dir);
        Ok(Workspace::from_core(CoreWorkspace::new(name), Some(dir)))
    }

    /// The workspace's name.
    #[getter]
    fn name(&self) -> String {
        self.data.lock().unwrap().inner.name.clone()
    }

    /// The workspace's id.
    #[getter]
    fn id(&self) -> String {
        self.data.lock().unwrap().inner.id.to_string()
    }

    /// The instruments, keyed by display name.
    #[getter]
    fn instruments(&self, py: Python<'_>) -> PyResult<PyObject> {
        let data = self.data.lock().unwrap();
        let dict = PyDict::new(py);
        for instrument in &data.inner.instruments {
            let handle = Py::new(
                py,
                WorkspaceInstrument {
                    data: self.data.clone(),
                    id: instrument.id.clone(),
                    name: instrument.label(),
                },
            )?;
            dict.set_item(instrument.label(), handle)?;
        }
        Ok(dict.into())
    }

    /// Look up an instrument by name or id.
    fn instrument(&self, py: Python<'_>, name: &str) -> PyResult<Py<WorkspaceInstrument>> {
        let data = self.data.lock().unwrap();
        let id = data
            .resolve_id(name)
            .ok_or_else(|| PyKeyError::new_err(format!("no instrument '{name}'")))?;
        let label = data
            .inner
            .instrument(&id)
            .map(|instrument| instrument.label())
            .unwrap_or_else(|| name.to_string());
        drop(data);
        Py::new(
            py,
            WorkspaceInstrument {
                data: self.data.clone(),
                id,
                name: label,
            },
        )
    }

    /// Add an instrument and connect it, returning its handle.
    #[pyo3(signature = (address, transport="vxi11", port=None, timeout=3.0))]
    fn add_instrument(
        &self,
        py: Python<'_>,
        address: &str,
        transport: &str,
        port: Option<u16>,
        timeout: f64,
    ) -> PyResult<Py<WorkspaceInstrument>> {
        let kind = match transport {
            "vxi11" => Transport::Vxi11,
            "raw" => Transport::Raw,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown transport '{other}' (expected 'vxi11' or 'raw')"
                )));
            }
        };
        let ip = parse_address(address).map_err(to_pyerr)?;
        let mut device = omariglol_core::device::Device::manual(ip);
        match kind {
            Transport::Vxi11 => device.vxi11_port = port,
            Transport::Raw => device.raw_port = port,
        }

        let mut data = self.data.lock().unwrap();
        let id = data.inner.add_instrument(device);
        if let Some(instrument) = data.inner.instrument_mut(&id) {
            instrument.config.transport = kind;
            instrument.config.port = port;
            instrument.config.timeout_secs = timeout.max(0.0) as u64;
        }
        let target = {
            let instrument = data.inner.instrument(&id).expect("just added");
            let mut target = Target::new(instrument.device.address, kind)
                .with_timeout(std::time::Duration::from_secs_f64(timeout.max(0.0)));
            if let Some(port) = port {
                target = target.with_port(port);
            }
            target
        };
        let mut session = Session::open(target).map_err(to_pyerr)?;
        let idn = session.identify().map_err(to_pyerr)?;
        data.sessions.insert(id.clone(), session);
        if let Some(instrument) = data.inner.instrument_mut(&id) {
            instrument.mark_connected(idn.clone(), now_millis());
        }
        let label = data
            .inner
            .instrument(&id)
            .map(|instrument| instrument.label())
            .unwrap_or_else(|| address.to_string());
        drop(data);
        Py::new(
            py,
            WorkspaceInstrument {
                data: self.data.clone(),
                id,
                name: label,
            },
        )
    }

    /// Connect an instrument that is already in the workspace.
    fn connect(&self, name: &str) -> PyResult<()> {
        let mut data = self.data.lock().unwrap();
        let id = data
            .resolve_id(name)
            .ok_or_else(|| PyKeyError::new_err(format!("no instrument '{name}'")))?;
        let target = {
            let instrument = data.inner.instrument(&id).expect("resolved above");
            let mut target = Target::new(instrument.device.address, instrument.config.transport)
                .with_timeout(instrument.config.timeout());
            if let Some(port) = instrument.config.port {
                target = target.with_port(port);
            }
            target
        };
        let mut session = Session::open(target).map_err(to_pyerr)?;
        let idn = session.identify().map_err(to_pyerr)?;
        data.sessions.insert(id.clone(), session);
        if let Some(instrument) = data.inner.instrument_mut(&id) {
            instrument.mark_connected(idn, now_millis());
        }
        Ok(())
    }

    /// Disconnect an instrument, keeping it in the workspace.
    fn disconnect(&self, name: &str) -> PyResult<()> {
        let mut data = self.data.lock().unwrap();
        let id = data
            .resolve_id(name)
            .ok_or_else(|| PyKeyError::new_err(format!("no instrument '{name}'")))?;
        if let Some(mut session) = data.sessions.remove(&id) {
            let _ = session.close();
        }
        if let Some(instrument) = data.inner.instrument_mut(&id) {
            instrument.mark_disconnected();
        }
        Ok(())
    }

    /// Query several instruments at once.
    ///
    /// `mapping` maps an instrument (or its name) to a query string.
    fn batch_query(&self, py: Python<'_>, mapping: Bound<'_, PyDict>) -> PyResult<PyObject> {
        let result = PyDict::new(py);
        for (key, value) in mapping.iter() {
            let id = resolve_instrument(&self.data, &key)?;
            let query: String = value.extract()?;
            let mut data = self.data.lock().unwrap();
            let session = data
                .sessions
                .get_mut(&id)
                .ok_or_else(|| PyRuntimeError::new_err("instrument is not connected"))?;
            let response = session.query(&query).map_err(to_pyerr)?;
            data.record(
                Some(id),
                TimelineEventKind::Query,
                format!("{query} -> {response}"),
            );
            result.set_item(key, response)?;
        }
        Ok(result.into())
    }

    /// Write to several instruments at once.
    fn batch_write(&self, mapping: Bound<'_, PyDict>) -> PyResult<()> {
        for (key, value) in mapping.iter() {
            let id = resolve_instrument(&self.data, &key)?;
            let command: String = value.extract()?;
            let mut data = self.data.lock().unwrap();
            let session = data
                .sessions
                .get_mut(&id)
                .ok_or_else(|| PyRuntimeError::new_err("instrument is not connected"))?;
            session.send(&command).map_err(to_pyerr)?;
            data.record(Some(id), TimelineEventKind::Command, command);
        }
        Ok(())
    }

    /// Apply a preset by name, sending each command to its instrument.
    fn apply_preset(&self, name: &str) -> PyResult<usize> {
        let mut data = self.data.lock().unwrap();
        let preset = data
            .inner
            .presets
            .iter()
            .find(|preset| preset.name == name)
            .cloned()
            .ok_or_else(|| PyKeyError::new_err(format!("no preset '{name}'")))?;
        let plan = preset.plan(&data.inner.instrument_ids());
        let mut sent = 0;
        for step in &plan.steps {
            if let Some(session) = data.sessions.get_mut(&step.instrument) {
                session.send(&step.command).map_err(to_pyerr)?;
                sent += 1;
            }
        }
        data.record(
            None,
            TimelineEventKind::Preset,
            format!("applied preset '{name}' ({sent} steps)"),
        );
        Ok(sent)
    }

    /// Define a preset from a mapping of instrument name to command list.
    fn define_preset(&self, name: &str, mapping: Bound<'_, PyDict>) -> PyResult<()> {
        let mut data = self.data.lock().unwrap();
        let mut preset = Preset::new(format!("preset-{}", now_millis()), name);
        for (key, value) in mapping.iter() {
            let id = resolve_instrument(&self.data, &key)?;
            let commands: Vec<String> = value.extract()?;
            for command in commands {
                preset.add_command(id.clone(), command);
            }
        }
        data.inner.add_preset(preset);
        Ok(())
    }

    /// Define a conditional routing rule.
    #[pyo3(signature = (name, source, query, threshold, target, action, comparison=">", fallback=None))]
    #[allow(clippy::too_many_arguments)]
    fn define_rule(
        &self,
        name: &str,
        source: Bound<'_, PyAny>,
        query: &str,
        threshold: f64,
        target: Bound<'_, PyAny>,
        action: &str,
        comparison: &str,
        fallback: Option<&str>,
    ) -> PyResult<()> {
        let source_id = resolve_instrument(&self.data, &source)?;
        let target_id = resolve_instrument(&self.data, &target)?;
        let comparison = match comparison {
            ">" => Comparison::GreaterThan,
            ">=" => Comparison::GreaterOrEqual,
            "<" => Comparison::LessThan,
            "<=" => Comparison::LessOrEqual,
            "==" => Comparison::Equal,
            "!=" => Comparison::NotEqual,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown comparison '{other}'"
                )));
            }
        };
        let mut rule = Rule::new(
            format!("rule-{}", now_millis()),
            name,
            source_id,
            target_id,
        );
        rule.query = query.to_string();
        rule.comparison = comparison;
        rule.threshold = threshold;
        rule.action = action.to_string();
        rule.fallback = fallback.map(str::to_string);
        rule.mode = EvaluationMode::Continuous;
        self.data.lock().unwrap().inner.add_rule(rule);
        Ok(())
    }

    /// Define a trigger edge between two instruments.
    fn define_trigger(
        &self,
        name: &str,
        source: Bound<'_, PyAny>,
        target: Bound<'_, PyAny>,
        condition: &str,
        action: &str,
    ) -> PyResult<()> {
        let source_id = resolve_instrument(&self.data, &source)?;
        let target_id = resolve_instrument(&self.data, &target)?;
        let trigger = Trigger::new(
            format!("trg-{}", now_millis()),
            source_id,
            target_id,
            condition,
            action,
        );
        self.data
            .lock()
            .unwrap()
            .inner
            .add_trigger(trigger)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let _ = name;
        Ok(())
    }

    /// Start a recorded automation run, used as a context manager.
    fn run(&self, name: &str) -> WorkspaceRun {
        {
            let mut data = self.data.lock().unwrap();
            data.record(
                None,
                TimelineEventKind::Script,
                format!("run '{name}' started"),
            );
        }
        WorkspaceRun {
            data: self.data.clone(),
            name: name.to_string(),
            last: None,
        }
    }

    /// Record a free-form marker on the timeline.
    fn record(&self, message: &str) -> PyResult<()> {
        let mut data = self.data.lock().unwrap();
        data.record(None, TimelineEventKind::Marker, message.to_string());
        Ok(())
    }

    /// Run discovery and return the instruments found.
    ///
    /// Uses the workspace's own cache, so a repeated call with
    /// `incremental=True` only probes what is new or expired.
    #[pyo3(signature = (use_broadcast=true, use_mdns=true, scan_subnet=true, extra_hosts=None, timeout=3.0, ttl=300.0, incremental=true))]
    #[allow(clippy::too_many_arguments)]
    fn discover(
        &self,
        py: Python<'_>,
        use_broadcast: bool,
        use_mdns: bool,
        scan_subnet: bool,
        extra_hosts: Option<Vec<String>>,
        timeout: f64,
        ttl: f64,
        incremental: bool,
    ) -> PyResult<PyObject> {
        let extra: Vec<std::net::IpAddr> = extra_hosts
            .unwrap_or_default()
            .iter()
            .filter_map(|host| host.parse().ok())
            .collect();
        let options = DiscoveryOptions {
            use_broadcast,
            use_mdns,
            scan_subnet,
            extra_hosts: extra,
            ttl_secs: ttl.max(0.0) as u64,
            incremental,
            listen_timeout: std::time::Duration::from_secs_f64(timeout.max(0.0)),
            ..Default::default()
        };
        let engine = self.data.lock().unwrap().discovery.clone();
        let report = engine.run(&options, |_| {});
        {
            let mut data = self.data.lock().unwrap();
            data.record(
                None,
                TimelineEventKind::Marker,
                format!(
                    "discover: {} found in {:.0} ms",
                    report.found.len(),
                    report.elapsed_ms
                ),
            );
        }
        let list = PyList::empty(py);
        for instrument in &report.found {
            list.append(cached_to_dict(py, instrument)?)?;
        }
        Ok(list.into())
    }

    /// Every cached discovery result, fresh or not.
    fn available_instruments(&self, py: Python<'_>) -> PyResult<PyObject> {
        let engine = self.data.lock().unwrap().discovery.clone();
        let cache = engine.cache();
        let guard = cache.lock().unwrap();
        let list = PyList::empty(py);
        for instrument in guard.all() {
            list.append(cached_to_dict(py, instrument)?)?;
        }
        Ok(list.into())
    }

    /// Cached instruments whose model contains `needle`.
    fn find_by_model(&self, py: Python<'_>, needle: &str) -> PyResult<PyObject> {
        let engine = self.data.lock().unwrap().discovery.clone();
        let cache = engine.cache();
        let guard = cache.lock().unwrap();
        let list = PyList::empty(py);
        for instrument in guard.find_by_model(needle) {
            list.append(cached_to_dict(py, instrument)?)?;
        }
        Ok(list.into())
    }

    /// Cached instruments that expose a named capability.
    fn find_by_capability(&self, py: Python<'_>, capability: &str) -> PyResult<PyObject> {
        let engine = self.data.lock().unwrap().discovery.clone();
        let cache = engine.cache();
        let guard = cache.lock().unwrap();
        let list = PyList::empty(py);
        for instrument in guard.find_by_capability(capability) {
            list.append(cached_to_dict(py, instrument)?)?;
        }
        Ok(list.into())
    }

    /// Bind a friendly name to an endpoint, adding and connecting it.
    #[pyo3(signature = (name, address, transport="vxi11", port=None, timeout=3.0))]
    fn bind(
        &self,
        py: Python<'_>,
        name: &str,
        address: &str,
        transport: &str,
        port: Option<u16>,
        timeout: f64,
    ) -> PyResult<Py<WorkspaceInstrument>> {
        let handle = self.add_instrument(py, address, transport, port, timeout)?;
        let id = handle.borrow(py).id.clone();
        self.data
            .lock()
            .unwrap()
            .aliases
            .insert(name.to_string(), id);
        Ok(handle)
    }

    /// The detected network configuration, as a dict.
    fn network_config(&self, py: Python<'_>) -> PyResult<PyObject> {
        let config = omariglol_core::netconfig::detect();
        let dict = PyDict::new(py);
        if let Some(primary) = &config.primary {
            dict.set_item("address", primary.address.to_string())?;
            dict.set_item("prefix_len", primary.prefix_len)?;
            dict.set_item("netmask", primary.netmask.to_string())?;
            dict.set_item("network", primary.network.to_string())?;
            dict.set_item("broadcast", primary.broadcast.to_string())?;
            dict.set_item("interface", &primary.name)?;
            dict.set_item("vlan", primary.vlan)?;
            dict.set_item("is_link_local", primary.is_link_local)?;
        }
        dict.set_item("gateway", config.gateway.map(|gateway| gateway.to_string()))?;
        dict.set_item("online", config.is_online())?;
        Ok(dict.into())
    }

    /// The number of events on the timeline.
    #[getter]
    fn timeline_len(&self) -> usize {
        self.data.lock().unwrap().inner.timeline.len()
    }

    /// Persist the workspace to its directory.
    fn save(&self) -> PyResult<()> {
        let data = self.data.lock().unwrap();
        let Some(dir) = &data.dir else {
            return Ok(());
        };
        std::fs::create_dir_all(dir).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let path = dir.join(format!("{}.json", data.inner.id.as_str()));
        let json = serde_json::to_string_pretty(&data.inner)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    fn __repr__(&self) -> String {
        let data = self.data.lock().unwrap();
        format!(
            "<omariglol.Workspace '{}' ({} instruments)>",
            data.inner.name,
            data.inner.instruments.len()
        )
    }
}

/// Convert a cached discovery result into a Python dict.
fn cached_to_dict<'py>(
    py: Python<'py>,
    instrument: &CachedInstrument,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("address", instrument.address().to_string())?;
    dict.set_item("idn", &instrument.idn)?;
    dict.set_item("vendor", &instrument.vendor)?;
    dict.set_item("model", &instrument.model)?;
    dict.set_item("serial", &instrument.serial)?;
    dict.set_item("capabilities", instrument.capabilities.clone())?;
    dict.set_item("vlan", instrument.vlan)?;
    dict.set_item("method", instrument.device.method.to_string())?;
    dict.set_item("raw_port", instrument.device.raw_port)?;
    dict.set_item("vxi11_port", instrument.device.vxi11_port)?;
    Ok(dict)
}

/// Resolve a Python object (an instrument handle or a name) to an instrument id.
fn resolve_instrument(
    data: &Arc<Mutex<WorkspaceData>>,
    obj: &Bound<'_, PyAny>,
) -> PyResult<InstrumentId> {
    if let Ok(handle) = obj.extract::<PyRef<'_, WorkspaceInstrument>>() {
        return Ok(handle.id.clone());
    }
    if let Ok(name) = obj.extract::<String>() {
        let guard = data.lock().unwrap();
        return guard
            .resolve_id(&name)
            .ok_or_else(|| PyKeyError::new_err(format!("no instrument '{name}'")));
    }
    Err(PyValueError::new_err(
        "expected an instrument handle or a name",
    ))
}

/// Convert a Python value into a JSON value, for timeline payloads.
fn python_to_json(value: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if value.is_none() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(b) = value.extract::<bool>() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Ok(i) = value.extract::<i64>() {
        return Ok(serde_json::Value::Number(i.into()));
    }
    if let Ok(f) = value.extract::<f64>() {
        return Ok(serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null));
    }
    if let Ok(s) = value.extract::<String>() {
        return Ok(serde_json::Value::String(s));
    }
    Ok(serde_json::Value::String(value.str()?.to_string_lossy().into_owned()))
}

/// Register the workspace classes with the `omariglol` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Workspace>()?;
    m.add_class::<WorkspaceInstrument>()?;
    m.add_class::<WorkspaceRun>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_dir_ends_with_workspaces() {
        let dir = default_dir();
        assert!(dir.ends_with("workspaces"));
    }

    #[test]
    fn test_load_workspace_roundtrip() {
        let dir = std::env::temp_dir().join(format!("omariglol-script-{}", now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let workspace = CoreWorkspace::new("Script Test");
        let path = dir.join(format!("{}.json", workspace.id.as_str()));
        std::fs::write(&path, serde_json::to_string(&workspace).unwrap()).unwrap();

        let loaded = load_workspace(&dir, "Script Test").unwrap();
        assert_eq!(loaded.name, "Script Test");
        // Also resolvable by id.
        let by_id = load_workspace(&dir, workspace.id.as_str()).unwrap();
        assert_eq!(by_id.id, workspace.id);
        assert!(load_workspace(&dir, "missing").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_python_to_json_scalars() {
        Python::with_gil(|py| {
            let value = python_to_json(&1i64.into_pyobject(py).unwrap().into_any()).unwrap();
            assert_eq!(value, serde_json::json!(1));
            let value = python_to_json(&"hi".into_pyobject(py).unwrap().into_any()).unwrap();
            assert_eq!(value, serde_json::json!("hi"));
        });
    }
}
