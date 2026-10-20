//! The `omariglol` Python module.
//!
//! This is the surface scripts actually touch: `connect()`, the instrument
//! object it returns, the progress helpers (`sample`, `status`, `checkpoint`)
//! and the CSV logger. Everything here is a thin shim over `omariglol-core`;
//! the interesting policy (cancellation, timeouts, event routing) lives in
//! [`crate::context`].

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Duration;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule};

use omariglol_core::device::{Target, Transport};
use omariglol_core::transport::{Session, parse_address};

use crate::context;
use crate::{ScriptError, ScriptEvent};

create_exception!(omariglol, ScriptCancelled, PyException);
create_exception!(omariglol, ScriptTimeout, PyException);

/// Translate a core error into a Python exception.
fn to_pyerr(err: omariglol_core::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

/// Translate a script error into a Python exception, preserving cancellation
/// and timeout so the engine can classify them when they surface.
pub(crate) fn script_err_to_py(err: ScriptError) -> PyErr {
    match err {
        ScriptError::Cancelled => ScriptCancelled::new_err("script cancelled"),
        ScriptError::Timeout(limit) => {
            ScriptTimeout::new_err(format!("script exceeded its time limit of {limit:?}"))
        }
        other => PyRuntimeError::new_err(other.to_string()),
    }
}

/// A live connection to one instrument, as seen from Python.
#[pyclass]
pub struct Instrument {
    session: Session,
    timeout: Duration,
}

#[pymethods]
impl Instrument {
    /// Write a command, adding a terminator if the caller omitted one.
    fn write(&mut self, command: &str) -> PyResult<()> {
        context::checkpoint().map_err(script_err_to_py)?;
        self.session.send(command).map_err(to_pyerr)
    }

    /// Send a query and return the response as a trimmed string.
    fn query(&mut self, command: &str) -> PyResult<String> {
        context::checkpoint().map_err(script_err_to_py)?;
        self.session.query(command).map_err(to_pyerr)
    }

    /// Send a query and return the raw response bytes.
    fn query_raw<'py>(&mut self, py: Python<'py>, command: &str) -> PyResult<Bound<'py, PyBytes>> {
        context::checkpoint().map_err(script_err_to_py)?;
        let bytes = self.session.query_raw(command).map_err(to_pyerr)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Identify the instrument with `*IDN?`.
    fn idn(&mut self) -> PyResult<String> {
        context::checkpoint().map_err(script_err_to_py)?;
        self.session.identify().map_err(to_pyerr)
    }

    /// The socket timeout, in seconds.
    #[getter]
    fn timeout(&self) -> f64 {
        self.timeout.as_secs_f64()
    }

    /// Change the socket timeout, in seconds.
    #[setter]
    fn set_timeout(&mut self, seconds: f64) -> PyResult<()> {
        let timeout = Duration::from_secs_f64(seconds.max(0.0));
        self.session.set_timeout(timeout).map_err(to_pyerr)?;
        self.timeout = timeout;
        Ok(())
    }

    /// Release the link.
    fn close(&mut self) -> PyResult<()> {
        self.session.close().map_err(to_pyerr)
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
        self.close()?;
        Ok(false)
    }

    fn __repr__(&self) -> String {
        format!("<omariglol.Instrument {}>", self.session.description())
    }
}

/// Open a connection to an instrument.
///
/// `transport` is `"vxi11"` (the default) or `"raw"`; `port` overrides the
/// transport default; `timeout` is in seconds.
#[pyfunction]
#[pyo3(signature = (address, transport=None, port=None, timeout=None))]
fn connect(
    address: &str,
    transport: Option<&str>,
    port: Option<u16>,
    timeout: Option<f64>,
) -> PyResult<Instrument> {
    let defaults = context::defaults().unwrap_or_else(|| context::ScriptDefaults {
        transport: Transport::Vxi11,
        timeout: Duration::from_secs(3),
        working_dir: None,
    });

    let kind = match transport {
        None => defaults.transport,
        Some("vxi11") => Transport::Vxi11,
        Some("raw") => Transport::Raw,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "unknown transport '{other}' (expected 'vxi11' or 'raw')"
            )));
        }
    };

    let ip = parse_address(address).map_err(to_pyerr)?;
    let timeout = timeout
        .map(|seconds| Duration::from_secs_f64(seconds.max(0.0)))
        .unwrap_or(defaults.timeout);

    let mut target = Target::new(ip, kind).with_timeout(timeout);
    if let Some(port) = port {
        target = target.with_port(port);
    }

    let session = Session::open(target).map_err(to_pyerr)?;
    Ok(Instrument { session, timeout })
}

/// Emit a named numeric sample, for live charts.
#[pyfunction]
fn sample(series: &str, value: f64) {
    context::emit(ScriptEvent::Sample {
        series: series.to_string(),
        value,
    });
}

/// Emit a human-readable status update.
#[pyfunction]
fn status(message: &str) {
    context::emit(ScriptEvent::Status {
        message: message.to_string(),
    });
}

/// Poll for cancellation and the wall-clock limit.
///
/// Long loops should call this so the script stays interruptible.
#[pyfunction]
fn checkpoint() -> PyResult<()> {
    context::checkpoint().map_err(script_err_to_py)
}

/// The working directory configured for the current run, if any.
#[pyfunction]
fn working_dir() -> Option<String> {
    context::defaults()
        .and_then(|defaults| defaults.working_dir)
        .map(|path| path.display().to_string())
}

/// Append-only CSV writer exposed to scripts.
#[pyclass]
pub struct CsvLogger {
    writer: BufWriter<File>,
    closed: bool,
}

#[pymethods]
impl CsvLogger {
    #[new]
    fn new(path: PathBuf, fields: Vec<String>) -> PyResult<Self> {
        let file = File::create(&path).map_err(|err| {
            PyRuntimeError::new_err(format!("could not create '{}': {err}", path.display()))
        })?;
        let mut writer = BufWriter::new(file);
        let header = fields
            .iter()
            .map(|field| csv_field(field))
            .collect::<Vec<_>>()
            .join(",");
        writeln!(writer, "{header}").map_err(io_pyerr)?;
        Ok(Self {
            writer,
            closed: false,
        })
    }

    /// Append one row. Values are stringified and CSV-escaped.
    #[pyo3(signature = (*values))]
    fn log(&mut self, py: Python<'_>, values: Vec<PyObject>) -> PyResult<()> {
        if self.closed {
            return Err(PyRuntimeError::new_err("CSV logger is closed"));
        }
        let mut cells = Vec::with_capacity(values.len());
        for value in values {
            let text = value.bind(py).str()?.to_string_lossy().into_owned();
            cells.push(csv_field(&text));
        }
        writeln!(self.writer, "{}", cells.join(",")).map_err(io_pyerr)?;
        Ok(())
    }

    /// Flush and close the file.
    fn close(&mut self) -> PyResult<()> {
        if !self.closed {
            self.writer.flush().map_err(io_pyerr)?;
            self.closed = true;
        }
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
        self.close()?;
        Ok(false)
    }
}

/// Wrap an I/O failure as a Python exception.
fn io_pyerr(err: std::io::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

/// Quote a CSV field when it contains a delimiter, quote or newline.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Which standard stream a write came from.
#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// A file-like object that forwards writes to the event sink.
///
/// Installed as `sys.stdout`/`sys.stderr` for the duration of a run, so
/// `print()` output streams back as [`ScriptEvent`]s before the script
/// finishes rather than being buffered until the end.
#[pyclass]
pub struct EventWriter {
    stream: Stream,
    text: String,
}

impl EventWriter {
    /// A writer that reports as stdout.
    pub fn stdout() -> Self {
        Self {
            stream: Stream::Stdout,
            text: String::new(),
        }
    }

    /// A writer that reports as stderr.
    pub fn stderr() -> Self {
        Self {
            stream: Stream::Stderr,
            text: String::new(),
        }
    }

    /// Everything written so far.
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[pymethods]
impl EventWriter {
    fn write(&mut self, text: &str) -> usize {
        self.text.push_str(text);
        let event = match self.stream {
            Stream::Stdout => ScriptEvent::Stdout {
                text: text.to_string(),
            },
            Stream::Stderr => ScriptEvent::Stderr {
                text: text.to_string(),
            },
        };
        context::emit(event);
        text.len()
    }

    fn flush(&self) {}

    fn isatty(&self) -> bool {
        false
    }

    fn writable(&self) -> bool {
        true
    }
}

/// The `omariglol` Python module.
#[pymodule]
pub fn omariglol(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(connect, m)?)?;
    m.add_function(wrap_pyfunction!(sample, m)?)?;
    m.add_function(wrap_pyfunction!(status, m)?)?;
    m.add_function(wrap_pyfunction!(checkpoint, m)?)?;
    m.add_function(wrap_pyfunction!(working_dir, m)?)?;
    m.add_class::<Instrument>()?;
    m.add_class::<CsvLogger>()?;
    // The multi-instrument workspace API (concepts.md §5.2).
    crate::workspace::register(m)?;
    m.add("__version__", omariglol_core::VERSION)?;
    m.add("ScriptCancelled", m.py().get_type::<ScriptCancelled>())?;
    m.add("ScriptTimeout", m.py().get_type::<ScriptTimeout>())?;
    Ok(())
}
