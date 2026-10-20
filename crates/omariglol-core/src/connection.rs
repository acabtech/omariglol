//! Workspace-scoped connection lifecycle (concepts.md §4.1, §2.3).
//!
//! The core [`Session`](crate::transport::Session) is the transport layer: it
//! owns a socket and speaks SCPI. A [`Connection`] wraps a session with the
//! workspace-scoped concerns the platform adds — a stable instrument id, retry
//! bookkeeping, and a place to record the last error — without performing any
//! event emission itself (that is the caller's job, so this stays UI-agnostic).

use std::time::Duration;

use crate::device::Target;
use crate::error::Result;
use crate::instrument::InstrumentId;
use crate::transport::Session;

/// The default interval between automatic reconnection attempts, in seconds.
pub const DEFAULT_RETRY_INTERVAL_SECS: u64 = 30;

/// A live transport-level link to one instrument, with retry bookkeeping.
pub struct Connection {
    /// The instrument this connection serves.
    pub instrument_id: InstrumentId,
    /// Where the connection dials.
    pub target: Target,
    /// The open session, or `None` when closed.
    session: Option<Session>,
    /// How many consecutive failures have occurred since the last success.
    pub retry_count: u32,
    /// The last error message, when the connection is failing.
    pub last_error: Option<String>,
}

impl Connection {
    /// Build a connection for a target, without opening it.
    pub fn new(instrument_id: InstrumentId, target: Target) -> Self {
        Connection {
            instrument_id,
            target,
            session: None,
            retry_count: 0,
            last_error: None,
        }
    }

    /// Open the connection, replacing any existing session.
    pub fn open(&mut self) -> Result<()> {
        let session = Session::open(self.target.clone())?;
        self.session = Some(session);
        self.retry_count = 0;
        self.last_error = None;
        Ok(())
    }

    /// Close the connection, if open.
    pub fn close(&mut self) {
        if let Some(mut session) = self.session.take() {
            let _ = session.close();
        }
    }

    /// True when a session is open.
    pub fn is_open(&self) -> bool {
        self.session.is_some()
    }

    /// The open session, or an error when closed.
    pub fn session_mut(&mut self) -> Result<&mut Session> {
        self.session
            .as_mut()
            .ok_or_else(|| crate::error::Error::Other("connection is not open".to_string()))
    }

    /// Send a query through the open session.
    pub fn query(&mut self, command: &str) -> Result<String> {
        self.session_mut()?.query(command)
    }

    /// Send a command through the open session.
    pub fn send(&mut self, command: &str) -> Result<()> {
        self.session_mut()?.send(command)
    }

    /// Identify the instrument through the open session.
    pub fn identify(&mut self) -> Result<String> {
        self.session_mut()?.identify()
    }

    /// Record a failure, incrementing the retry counter.
    pub fn record_failure(&mut self, message: impl Into<String>) {
        self.retry_count = self.retry_count.saturating_add(1);
        self.last_error = Some(message.into());
    }

    /// Record a success, clearing the retry counter and last error.
    pub fn record_success(&mut self) {
        self.retry_count = 0;
        self.last_error = None;
    }

    /// True when the retry window has elapsed and a reconnect should be tried.
    ///
    /// `elapsed_since_last_attempt` is how long ago the last attempt was made.
    pub fn should_retry(&self, elapsed_since_last_attempt: Duration, interval_secs: u64) -> bool {
        !self.is_open() && elapsed_since_last_attempt >= Duration::from_secs(interval_secs)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Transport;
    use std::net::IpAddr;

    fn target() -> Target {
        Target::new("127.0.0.1".parse::<IpAddr>().unwrap(), Transport::Raw)
    }

    #[test]
    fn test_connection_starts_closed() {
        let connection = Connection::new(InstrumentId::new("a"), target());
        assert!(!connection.is_open());
        assert_eq!(connection.retry_count, 0);
        assert!(connection.last_error.is_none());
    }

    #[test]
    fn test_session_mut_errors_when_closed() {
        let mut connection = Connection::new(InstrumentId::new("a"), target());
        assert!(connection.session_mut().is_err());
        assert!(connection.query("*IDN?").is_err());
    }

    #[test]
    fn test_record_failure_and_success() {
        let mut connection = Connection::new(InstrumentId::new("a"), target());
        connection.record_failure("timed out");
        connection.record_failure("timed out again");
        assert_eq!(connection.retry_count, 2);
        assert_eq!(connection.last_error.as_deref(), Some("timed out again"));
        connection.record_success();
        assert_eq!(connection.retry_count, 0);
        assert!(connection.last_error.is_none());
    }

    #[test]
    fn test_should_retry() {
        let connection = Connection::new(InstrumentId::new("a"), target());
        assert!(connection.should_retry(Duration::from_secs(31), 30));
        assert!(!connection.should_retry(Duration::from_secs(10), 30));
    }

    #[test]
    fn test_close_is_idempotent() {
        let mut connection = Connection::new(InstrumentId::new("a"), target());
        connection.close();
        connection.close();
        assert!(!connection.is_open());
    }
}
