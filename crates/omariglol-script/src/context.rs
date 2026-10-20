//! Per-run state shared between the Rust engine and the Python shim.
//!
//! The `omariglol` Python module is registered once and lives for the process
//! lifetime, so it cannot capture the per-run callback, cancellation flag or
//! defaults directly. Instead the engine installs them in thread-locals for the
//! duration of a run and the shim reads them back. Scripts execute on the
//! calling thread, so a thread-local is exactly the right scope: two engines
//! running on two threads never see each other's state.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use omariglol_core::device::Transport;

use crate::{ScriptError, ScriptEvent};

/// Defaults handed to `connect()` when a script does not name a transport.
#[derive(Debug, Clone)]
pub struct ScriptDefaults {
    /// Transport used when `connect()` is called without one.
    pub transport: Transport,
    /// Socket timeout handed to instruments opened by the script.
    pub timeout: Duration,
    /// Working directory for relative file access.
    pub working_dir: Option<PathBuf>,
}

/// The per-run event callback installed by the engine.
type EventSink = Box<dyn FnMut(ScriptEvent)>;

thread_local! {
    static SINK: RefCell<Option<EventSink>> = const { RefCell::new(None) };
    static CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
    static DEADLINE: Cell<Option<(Instant, Duration)>> = const { Cell::new(None) };
    static DEFAULTS: RefCell<Option<ScriptDefaults>> = const { RefCell::new(None) };
}

/// Install the event sink for the current run.
pub fn set_sink(sink: Box<dyn FnMut(ScriptEvent)>) {
    SINK.with(|cell| *cell.borrow_mut() = Some(sink));
}

/// Remove the event sink once the run is over.
pub fn clear_sink() {
    SINK.with(|cell| *cell.borrow_mut() = None);
}

/// Emit an event to the installed sink, if any.
pub fn emit(event: ScriptEvent) {
    SINK.with(|cell| {
        if let Some(sink) = cell.borrow_mut().as_mut() {
            sink(event);
        }
    });
}

/// Install the cancellation flag for the current run.
pub fn set_cancel(flag: Arc<AtomicBool>) {
    CANCEL.with(|cell| *cell.borrow_mut() = Some(flag));
}

/// Remove the cancellation flag once the run is over.
pub fn clear_cancel() {
    CANCEL.with(|cell| *cell.borrow_mut() = None);
}

/// True when the current run has been asked to stop.
pub fn is_cancelled() -> bool {
    CANCEL.with(|cell| {
        cell.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    })
}

/// Install the wall-clock deadline for the current run.
pub fn set_deadline(deadline: Option<(Instant, Duration)>) {
    DEADLINE.with(|cell| cell.set(deadline));
}

/// Install the defaults `connect()` falls back to.
pub fn set_defaults(defaults: ScriptDefaults) {
    DEFAULTS.with(|cell| *cell.borrow_mut() = Some(defaults));
}

/// Remove the defaults once the run is over.
pub fn clear_defaults() {
    DEFAULTS.with(|cell| *cell.borrow_mut() = None);
}

/// The defaults `connect()` falls back to, if a run is in progress.
pub fn defaults() -> Option<ScriptDefaults> {
    DEFAULTS.with(|cell| cell.borrow().clone())
}

/// Poll the cancellation flag and the wall-clock deadline.
///
/// The Python shim calls this before every instrument operation and from the
/// explicit `checkpoint()` helper, which is what makes a long script
/// interruptible.
pub fn checkpoint() -> Result<(), ScriptError> {
    if is_cancelled() {
        return Err(ScriptError::Cancelled);
    }
    DEADLINE.with(|cell| match cell.get() {
        Some((deadline, limit)) if Instant::now() >= deadline => Err(ScriptError::Timeout(limit)),
        _ => Ok(()),
    })
}
