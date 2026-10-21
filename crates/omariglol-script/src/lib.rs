//! # omariglol-script
//!
//! The embedded Python scripting engine. This is where `lxi-tools`' Lua lives,
//! replaced deliberately:
//!
//! * Engineers already automate in Python, so scripts are portable and the
//!   surrounding ecosystem (`numpy`, `matplotlib`, `pandas`) is available.
//! * PyO3 with `abi3-py310` produces one binary that works across Python
//!   3.10 and newer, with no per-version rebuild.
//!
//! ## Status
//!
//! **Implemented (M9).** The engine embeds CPython, registers the `omariglol`
//! module, streams `print()` output as [`ScriptEvent`]s, captures stdout/stderr,
//! duration and exit code, and supports cancellation and an optional wall-clock
//! limit.
//!
//! ## Python surface
//!
//! ```python
//! from omariglol import connect
//!
//! with connect("192.168.1.50") as inst:
//!     print(inst.idn())
//!     inst.write(":CHAN1:SCAL 0.5")
//!     freq = float(inst.query(":MEAS:FREQ?"))
//! ```
//!
//! `connect()` also accepts `transport="raw"`, `port=` and `timeout=`. The
//! module additionally exposes `sample(series, value)`, `status(message)`,
//! `checkpoint()` and the `CsvLogger` helper.

mod context;
mod module;
mod workspace;

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pyo3::exceptions::PySystemExit;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use omariglol_core::device::Transport;
use serde::{Deserialize, Serialize};

use context::ScriptDefaults;
use module::{EventWriter, ScriptCancelled, ScriptTimeout};

/// Errors raised by the scripting engine.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("could not read script '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("script raised an exception:\n{traceback}")]
    Python { traceback: String },

    #[error("script was cancelled")]
    Cancelled,

    #[error("script exceeded its time limit of {0:?}")]
    Timeout(Duration),

    #[error("the embedded interpreter is not available: {0}")]
    InterpreterUnavailable(String),

    #[error("{0}")]
    Other(String),
}

/// How to execute a script.
#[derive(Debug, Clone)]
pub struct ScriptOptions {
    /// Transport used when a script calls `connect()` without naming one.
    pub default_transport: Transport,
    /// Default socket timeout handed to instruments opened by the script.
    pub timeout: Duration,
    /// Optional wall-clock limit for the whole script.
    pub max_duration: Option<Duration>,
    /// Arguments exposed to the script as `sys.argv`.
    pub args: Vec<String>,
    /// Working directory for relative file access.
    pub working_dir: Option<PathBuf>,
}

impl Default for ScriptOptions {
    fn default() -> Self {
        ScriptOptions {
            default_transport: Transport::Vxi11,
            timeout: Duration::from_secs(3),
            max_duration: None,
            args: Vec::new(),
            working_dir: None,
        }
    }
}

/// A progress or output event emitted while a script runs.
///
/// The GUI subscribes to these to stream `print()` output and plot points
/// without waiting for the script to finish.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ScriptEvent {
    /// A line written by the script.
    Stdout { text: String },
    /// A line written to stderr.
    Stderr { text: String },
    /// A named numeric sample, for live charts.
    Sample { series: String, value: f64 },
    /// A human-readable status update.
    Status { message: String },
}

/// The outcome of a script run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptResult {
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// How long the script took.
    pub duration_ms: f64,
    /// The script's exit value, when it set one.
    pub exit_code: Option<i32>,
}

/// The embedded interpreter.
///
/// Held across runs so that imports are only paid for once, mirroring how a
/// REPL keeps its state.
pub struct ScriptEngine {
    /// Shared with the Python shim so `cancel()` can be called from any thread.
    cancel: Arc<AtomicBool>,
    /// Whether the `omariglol` module has been registered with `sys.modules`.
    module_registered: bool,
}

impl ScriptEngine {
    /// Create an engine, initialising the embedded interpreter and registering
    /// the `omariglol` module.
    pub fn new() -> Result<Self, ScriptError> {
        let mut engine = ScriptEngine {
            cancel: Arc::new(AtomicBool::new(false)),
            module_registered: false,
        };
        engine.ensure_module()?;
        Ok(engine)
    }

    /// Register the module once, on first use.
    fn ensure_module(&mut self) -> Result<(), ScriptError> {
        if !self.module_registered {
            register_module()?;
            self.module_registered = true;
        }
        Ok(())
    }

    /// Run a script file, reporting progress through `on_event`.
    ///
    /// The script path becomes `sys.argv[0]`, followed by `options.args`, as
    /// CPython does for `python script.py ...`.
    pub fn run_file(
        &mut self,
        path: &Path,
        options: &ScriptOptions,
        on_event: impl FnMut(ScriptEvent) + 'static,
    ) -> Result<ScriptResult, ScriptError> {
        let source = std::fs::read_to_string(path).map_err(|source| ScriptError::Read {
            path: path.to_path_buf(),
            source,
        })?;

        let mut options = options.clone();
        let mut args = vec![path.display().to_string()];
        args.extend(options.args.iter().cloned());
        options.args = args;

        self.run_source(&source, &options, on_event)
    }

    /// Run a script from a string — what the GUI's editor uses.
    pub fn run_source(
        &mut self,
        source: &str,
        options: &ScriptOptions,
        on_event: impl FnMut(ScriptEvent) + 'static,
    ) -> Result<ScriptResult, ScriptError> {
        self.ensure_module()?;
        self.cancel.store(false, Ordering::Relaxed);

        let deadline = options
            .max_duration
            .map(|limit| (Instant::now() + limit, limit));

        context::set_sink(Box::new(on_event));
        context::set_cancel(self.cancel.clone());
        context::set_deadline(deadline);
        context::set_defaults(ScriptDefaults {
            transport: options.default_transport,
            timeout: options.timeout,
            working_dir: options.working_dir.clone(),
        });

        let start = Instant::now();
        let outcome = Python::with_gil(|py| run_python(py, source, options));
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        context::clear_sink();
        context::clear_cancel();
        context::set_deadline(None);
        context::clear_defaults();

        let (stdout, stderr, exit_code) = outcome?;
        Ok(ScriptResult {
            stdout,
            stderr,
            duration_ms,
            exit_code,
        })
    }

    /// Ask a running script to stop at its next checkpoint.
    ///
    /// Safe to call from another thread while [`ScriptEngine::run_source`] is
    /// executing.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// A cloneable handle that can request cancellation from another thread.
    ///
    /// The GUI holds this so `cancel_script` can stop a run without waiting for
    /// the engine lock, which the running script holds for its whole duration.
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        // An engine that failed to initialise is a programming error, not a
        // recoverable condition, so panicking here is deliberate.
        Self::new().expect("failed to initialise the embedded interpreter")
    }
}

/// Register the `omariglol` Python module with the interpreter.
///
/// Exposes `connect()` returning an instrument object with `write`, `query`,
/// `query_raw`, `idn`, `timeout` and context-manager support.
pub fn register_module() -> Result<(), ScriptError> {
    Python::with_gil(|py| {
        let module = pyo3::wrap_pymodule!(module::omariglol)(py);
        let sys = py.import("sys").map_err(setup_err)?;
        sys.getattr("modules")
            .map_err(setup_err)?
            .set_item("omariglol", &module)
            .map_err(setup_err)?;
        Ok(())
    })
}

/// Execute `source` with stdout/stderr redirected to event writers.
fn run_python(
    py: Python<'_>,
    source: &str,
    options: &ScriptOptions,
) -> Result<(String, String, Option<i32>), ScriptError> {
    let sys = py.import("sys").map_err(setup_err)?;

    // sys.argv, as CPython would set it.
    let argv = PyList::new(py, options.args.iter().map(String::as_str)).map_err(setup_err)?;
    sys.setattr("argv", argv).map_err(setup_err)?;

    // Redirect stdout/stderr so print() streams back as events.
    let stdout = Py::new(py, EventWriter::stdout()).map_err(setup_err)?;
    let stderr = Py::new(py, EventWriter::stderr()).map_err(setup_err)?;
    let old_stdout = sys.getattr("stdout").map_err(setup_err)?;
    let old_stderr = sys.getattr("stderr").map_err(setup_err)?;
    sys.setattr("stdout", stdout.clone_ref(py))
        .map_err(setup_err)?;
    sys.setattr("stderr", stderr.clone_ref(py))
        .map_err(setup_err)?;

    let globals = PyDict::new(py);
    globals
        .set_item("__name__", "__main__")
        .map_err(setup_err)?;
    globals
        .set_item(
            "__file__",
            options.args.first().cloned().unwrap_or_default(),
        )
        .map_err(setup_err)?;
    globals
        .set_item("__builtins__", py.import("builtins").map_err(setup_err)?)
        .map_err(setup_err)?;

    let code = CString::new(source)
        .map_err(|_| ScriptError::Other("script contains a NUL byte".to_string()))?;
    let result = py.run(&code, Some(&globals), None);

    // Restore the real streams, whatever happened.
    let _ = sys.setattr("stdout", old_stdout);
    let _ = sys.setattr("stderr", old_stderr);

    let stdout_text = stdout.borrow(py).text().to_string();
    let stderr_text = stderr.borrow(py).text().to_string();

    match result {
        Ok(()) => Ok((stdout_text, stderr_text, None)),
        Err(err) => {
            if err.is_instance_of::<ScriptCancelled>(py) {
                return Err(ScriptError::Cancelled);
            }
            if err.is_instance_of::<ScriptTimeout>(py) {
                return Err(ScriptError::Timeout(
                    options.max_duration.unwrap_or_default(),
                ));
            }
            if let Some(code) = system_exit_code(py, &err) {
                return Ok((stdout_text, stderr_text, Some(code)));
            }
            Err(ScriptError::Python {
                traceback: format_traceback(py, &err),
            })
        }
    }
}

/// Extract the exit code from a `SystemExit`, if that is what was raised.
fn system_exit_code(py: Python<'_>, err: &PyErr) -> Option<i32> {
    if !err.is_instance_of::<PySystemExit>(py) {
        return None;
    }
    match err.value(py).getattr("code") {
        // SystemExit() and SystemExit(None) both mean success.
        Ok(code) if code.is_none() => Some(0),
        // SystemExit(int) is the code; SystemExit(str) prints and exits 1.
        Ok(code) => code.extract::<i32>().ok().or(Some(1)),
        Err(_) => Some(0),
    }
}

/// Render a Python exception as a full traceback, falling back to the short
/// form if the `traceback` module is unavailable.
fn format_traceback(py: Python<'_>, err: &PyErr) -> String {
    if let Ok(traceback) = py.import("traceback") {
        let args = (err.get_type(py), err.value(py).clone(), err.traceback(py));
        if let Ok(lines) = traceback.call_method1("format_exception", args) {
            if let Ok(text) = lines.extract::<Vec<String>>() {
                return text.concat();
            }
        }
    }
    err.to_string()
}

/// Wrap an interpreter setup failure.
fn setup_err(err: PyErr) -> ScriptError {
    ScriptError::Other(format!("failed to prepare the interpreter: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::Ordering;

    /// Run a snippet and collect the events it emitted.
    fn run(source: &str) -> (Result<ScriptResult, ScriptError>, Vec<ScriptEvent>) {
        let mut engine = ScriptEngine::new().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let result = engine.run_source(source, &ScriptOptions::default(), move |event| {
            sink.lock().unwrap().push(event);
        });
        let events = Arc::try_unwrap(events).unwrap().into_inner().unwrap();
        (result, events)
    }

    #[test]
    fn test_module_imports() {
        let (result, _) = run(
            "from omariglol import connect, sample, status, checkpoint, CsvLogger, working_dir\n",
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_workspace_api_imports() {
        let (result, _) =
            run("from omariglol import Workspace, WorkspaceInstrument, WorkspaceRun\n");
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_workspace_create_and_run() {
        let (result, _) = run("from omariglol import Workspace\n\
             ws = Workspace.create('test-ws')\n\
             assert ws.name == 'test-ws', ws.name\n\
             with ws.run('demo') as run:\n\
             \x20   run.record(step=1, value=2.5)\n\
             assert ws.timeline_len >= 2, ws.timeline_len\n\
             print('ok')\n");
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_workspace_define_preset_and_rule() {
        let (result, _) = run("from omariglol import Workspace\n\
             ws = Workspace.create('test-ws')\n\
             ws.define_preset('P', {'scope': [':RUN', ':STOP']})\n\
             ws.define_rule('R', 'scope', ':MEAS:FREQ?', 1000.0, 'scope', ':STOP')\n\
             print('ok')\n");
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_workspace_discovery_api() {
        let (result, _) = run("from omariglol import Workspace\n\
             ws = Workspace.create('test-ws')\n\
             cfg = ws.network_config()\n\
             assert 'online' in cfg, cfg\n\
             assert ws.available_instruments() == [], ws.available_instruments()\n\
             assert ws.find_by_model('ds1054') == []\n\
             assert ws.find_by_capability('run') == []\n\
             print('ok')\n");
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_run_source_captures_stdout() {
        let (result, _) = run("print('hello world')\n");
        let result = result.unwrap();
        assert!(result.stdout.contains("hello world"), "{:?}", result.stdout);
        assert_eq!(result.exit_code, None);
    }

    #[test]
    fn test_run_source_streams_events() {
        let (result, events) = run("print('streamed')\n");
        assert!(result.is_ok());
        let streamed = events.iter().any(
            |event| matches!(event, ScriptEvent::Stdout { text } if text.contains("streamed")),
        );
        assert!(streamed, "expected a stdout event, got {events:?}");
    }

    #[test]
    fn test_run_source_captures_stderr() {
        let (result, events) = run("import sys\nsys.stderr.write('oops\\n')\n");
        let result = result.unwrap();
        assert!(result.stderr.contains("oops"), "{:?}", result.stderr);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ScriptEvent::Stderr { .. }))
        );
    }

    #[test]
    fn test_run_source_sample_and_status() {
        let (result, events) =
            run("import omariglol\nomariglol.sample('freq', 1.5)\nomariglol.status('done')\n");
        assert!(result.is_ok());
        assert!(events.iter().any(|event| matches!(
            event,
            ScriptEvent::Sample { series, value } if series == "freq" && (*value - 1.5).abs() < f64::EPSILON
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ScriptEvent::Status { message } if message == "done"
        )));
    }

    #[test]
    fn test_run_source_traceback() {
        let (result, _) = run("raise ValueError('boom')\n");
        match result {
            Err(ScriptError::Python { traceback }) => {
                assert!(traceback.contains("ValueError"), "{traceback}");
                assert!(traceback.contains("boom"), "{traceback}");
            }
            other => panic!("expected a Python error, got {other:?}"),
        }
    }

    #[test]
    fn test_run_source_exit_code() {
        let (result, _) = run("import sys\nsys.exit(3)\n");
        assert_eq!(result.unwrap().exit_code, Some(3));
    }

    #[test]
    fn test_run_source_exit_zero() {
        let (result, _) = run("import sys\nsys.exit()\n");
        assert_eq!(result.unwrap().exit_code, Some(0));
    }

    #[test]
    fn test_run_source_timeout() {
        let mut engine = ScriptEngine::new().unwrap();
        let options = ScriptOptions {
            max_duration: Some(Duration::from_millis(100)),
            ..Default::default()
        };
        let source = "import omariglol\nfor _ in range(10_000_000):\n    omariglol.checkpoint()\n";
        let result = engine.run_source(source, &options, |_| {});
        assert!(matches!(result, Err(ScriptError::Timeout(_))), "{result:?}");
    }

    #[test]
    fn test_cancel_stops_script() {
        let mut engine = ScriptEngine::new().unwrap();
        let flag = engine.cancel.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            flag.store(true, Ordering::Relaxed);
        });
        let source = "import omariglol\nfor _ in range(10_000_000):\n    omariglol.checkpoint()\n";
        let result = engine.run_source(source, &ScriptOptions::default(), |_| {});
        handle.join().unwrap();
        assert!(matches!(result, Err(ScriptError::Cancelled)), "{result:?}");
    }

    #[test]
    fn test_csv_logger() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("omariglol-csv-{}.csv", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let source = format!(
            "import omariglol\n\
             with omariglol.CsvLogger(r'{}', ['freq', 'amp']) as log:\n\
             \x20   log.log(1.0, 2.0)\n\
             \x20   log.log('a,b', 'quote\"d')\n",
            path.display()
        );
        let (result, _) = run(&source);
        assert!(result.is_ok(), "{result:?}");

        let contents = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(contents.starts_with("freq,amp\n"), "{contents}");
        assert!(contents.contains("1.0,2.0"), "{contents}");
        assert!(contents.contains("\"a,b\""), "{contents}");
        assert!(contents.contains("\"quote\"\"d\""), "{contents}");
    }

    #[test]
    fn test_connect_unknown_transport() {
        let (result, _) =
            run("import omariglol\nomariglol.connect('127.0.0.1', transport='bogus')\n");
        match result {
            Err(ScriptError::Python { traceback }) => {
                assert!(traceback.contains("ValueError"), "{traceback}");
            }
            other => panic!("expected a ValueError, got {other:?}"),
        }
    }

    #[test]
    fn test_engine_reused_across_runs() {
        let mut engine = ScriptEngine::new().unwrap();
        for _ in 0..3 {
            let result = engine.run_source("print('again')\n", &ScriptOptions::default(), |_| {});
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_run_file_sets_argv() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("omariglol-argv-{}.py", std::process::id()));
        std::fs::write(&path, "import sys\nprint('|'.join(sys.argv))\n").unwrap();

        let mut engine = ScriptEngine::new().unwrap();
        let options = ScriptOptions {
            args: vec!["one".to_string(), "two".to_string()],
            ..Default::default()
        };
        let result = engine.run_file(&path, &options, |_| {}).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(result.stdout.contains(&path.display().to_string()));
        assert!(result.stdout.contains("one|two"), "{}", result.stdout);
    }

    #[test]
    fn test_run_file_missing() {
        let mut engine = ScriptEngine::new().unwrap();
        let result = engine.run_file(
            Path::new("/nonexistent/omariglol-script.py"),
            &ScriptOptions::default(),
            |_| {},
        );
        assert!(matches!(result, Err(ScriptError::Read { .. })));
    }
}
