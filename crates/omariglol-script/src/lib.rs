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
//! **Scaffold.** The public types and the module contract are in place; the
//! interpreter wiring is stubbed.
//!
//! ## Planned Python surface
//!
//! ```python
//! from omariglol import connect
//!
//! with connect("192.168.1.50") as inst:
//!     print(inst.idn())
//!     inst.write(":CHAN1:SCAL 0.5")
//!     freq = float(inst.query(":MEAS:FREQ?"))
//! ```

// Scaffold: the types below declare the surface their implementations will use.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use omariglol_core::device::Transport;
use serde::{Deserialize, Serialize};

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
    initialized: bool,
}

impl ScriptEngine {
    /// Create an engine, initialising the embedded interpreter lazily.
    pub fn new() -> Result<Self, ScriptError> {
        todo!("prepare the embedded CPython interpreter")
    }

    /// Run a script file, reporting progress through `on_event`.
    pub fn run_file(
        &mut self,
        path: &Path,
        options: &ScriptOptions,
        on_event: impl FnMut(ScriptEvent),
    ) -> Result<ScriptResult, ScriptError> {
        let _ = (path, options, on_event);
        todo!("read, execute and capture the script")
    }

    /// Run a script from a string — what the GUI's editor uses.
    pub fn run_source(
        &mut self,
        source: &str,
        options: &ScriptOptions,
        on_event: impl FnMut(ScriptEvent),
    ) -> Result<ScriptResult, ScriptError> {
        let _ = (source, options, on_event);
        todo!("execute in-memory source")
    }

    /// Ask a running script to stop at its next checkpoint.
    pub fn cancel(&self) {
        let _ = self;
        todo!("set the cancellation flag the instrument shim polls")
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
    todo!("build the omariglol Python module and add it to sys.modules")
}
