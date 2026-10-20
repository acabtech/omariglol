//! Transport abstraction shared by the CLI, the GUI and the shell plugin.

pub mod raw;
pub mod rpc;
pub mod vxi11;

use crate::device::Target;
use crate::error::Result;

/// A bidirectional SCPI channel to one instrument.
///
/// Implementations are deliberately blocking: instruments are slow, the
/// protocol is request/response, and callers (Tauri commands, CLI) already run
/// us off the UI thread.
pub trait ScpiTransport: Send {
    /// Write a command. A terminator is added if the caller omitted one.
    fn send(&mut self, command: &str) -> Result<()>;

    /// Read one response, with any transport terminator stripped.
    fn receive(&mut self) -> Result<Vec<u8>>;

    /// Write then read — the common SCPI query.
    fn query(&mut self, command: &str) -> Result<Vec<u8>> {
        let _ = command;
        todo!("send then receive")
    }

    /// Release the link. Called on drop paths where possible.
    fn close(&mut self) -> Result<()>;

    /// Human-readable description of the underlying channel.
    fn description(&self) -> String;
}

/// Open a transport for `target`.
pub fn open(target: &Target) -> Result<Box<dyn ScpiTransport>> {
    let _ = target;
    todo!("dispatch on Transport::Vxi11 / Transport::Raw")
}

/// A convenient wrapper that owns a transport and tracks the target, with
/// helpers for the text-oriented operations the UI needs.
pub struct Session {
    transport: Box<dyn ScpiTransport>,
    pub target: Target,
}

impl Session {
    pub fn open(target: Target) -> Result<Self> {
        let _ = target;
        todo!("open a transport and wrap it")
    }

    /// Send a command and discard any response.
    pub fn send(&mut self, command: &str) -> Result<()> {
        let _ = command;
        todo!("delegate to the transport")
    }

    /// Send a command and return the raw response bytes.
    pub fn query_raw(&mut self, command: &str) -> Result<Vec<u8>> {
        let _ = command;
        todo!("delegate to the transport")
    }

    /// Send a query and decode the response as a trimmed string, as
    /// `lxi scpi` prints it.
    pub fn query(&mut self, command: &str) -> Result<String> {
        let _ = command;
        todo!("delegate and decode as UTF-8")
    }

    /// Identify the instrument with `*IDN?`.
    pub fn identify(&mut self) -> Result<String> {
        todo!("query *IDN?")
    }

    pub fn description(&self) -> String {
        let _ = self;
        todo!("describe the underlying transport")
    }

    pub fn close(&mut self) -> Result<()> {
        todo!("close the transport")
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Best effort; a failure here is not actionable for the caller.
        todo!("close the transport on drop")
    }
}

/// Format raw response bytes the way `lxi scpi --hex` does.
pub fn to_hex(data: &[u8]) -> String {
    let _ = data;
    todo!("render bytes as space-separated hex")
}

/// Interpret an instrument address string ("192.168.1.10", "scope.local").
pub fn parse_address(input: &str) -> Result<std::net::IpAddr> {
    let _ = input;
    todo!("parse a literal IP, else resolve the hostname")
}
