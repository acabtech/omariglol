//! Error types for the omariglol LXI core.
//!
//! Every failure a frontend might need to explain to a user gets its own
//! variant. The frontends match on these rather than string-matching messages.

use std::io;
use std::net::SocketAddr;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong talking to an LXI instrument.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid address or port: {0}")]
    Addr(String),

    #[error("timed out waiting for the instrument ({0})")]
    Timeout(String),

    #[error("instrument returned VXI-11 error {code}: {message}")]
    Vxi11 { code: i32, message: String },

    #[error("ONC RPC protocol error: {0}")]
    Rpc(String),

    #[error("portmapper refused to map program 0x{program:06x}: {message}")]
    Portmap { program: u32, message: String },

    #[error("no LXI instrument found at {0}")]
    NotFound(SocketAddr),

    #[error("malformed response from instrument: {0}")]
    Malformed(String),

    #[error("instrument does not support screenshot: {0}")]
    ScreenshotUnsupported(String),

    #[error("operation not supported: {0}")]
    Unsupported(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// True when the failure is a timeout.
    ///
    /// Callers use this to distinguish "instrument is present but slow" from
    /// "instrument is not there", which changes what the UI should say.
    pub fn is_timeout(&self) -> bool {
        let _ = self;
        todo!("classify timeout-shaped errors")
    }
}

/// Human-readable name for a VXI-11 `Device_ErrorCode`.
///
/// Mirrors the strings liblxi/lxi-tools surface to users.
pub fn vxi11_error_name(code: i32) -> &'static str {
    match code {
        0 => "no error",
        1 => "syntax error",
        3 => "device not accessible",
        4 => "invalid link identifier",
        5 => "parameter error",
        6 => "channel not established",
        8 => "operation not supported",
        9 => "out of resources",
        11 => "device locked by another link",
        12 => "no lock held by this link",
        15 => "I/O timeout",
        17 => "I/O error",
        21 => "invalid address",
        23 => "abort",
        29 => "channel already established",
        _ => "unknown error",
    }
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn test_is_timeout_classifies_timeout() {
        todo!("classify timeout-shaped errors")
    }

    #[test]
    #[ignore]
    fn test_vxi11_error_name_maps_known_codes() {
        todo!("map Device_ErrorCode to its spec name")
    }
}
