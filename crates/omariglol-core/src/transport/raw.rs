//! Raw TCP SCPI transport (the `lxi scpi --raw` path).
//!
//! Instruments supporting the LXI "raw socket" convention listen on TCP 5025
//! and speak bare SCPI terminated by `\n`, with no RPC framing at all.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use super::ScpiTransport;
use crate::device::Target;
use crate::error::Result;

/// The IANA-assigned LXI raw socket port.
pub const DEFAULT_RAW_PORT: u16 = 5025;

/// Upper bound on a single raw response, so an instrument that never sends a
/// terminator cannot exhaust memory.
pub const MAX_RESPONSE: usize = 32 * 1024 * 1024;

/// A raw TCP connection to an instrument's SCPI socket.
pub struct RawTransport {
    stream: TcpStream,
    timeout: Duration,
    /// Bytes already read past the terminator, kept for the next receive.
    pending: Vec<u8>,
    addr: SocketAddr,
}

impl RawTransport {
    pub fn connect(target: &Target) -> Result<Self> {
        let _ = target;
        todo!("connect to the raw SCPI port")
    }

    /// The socket address this transport is connected to.
    pub fn peer(&self) -> SocketAddr {
        let _ = self;
        todo!("return the peer address")
    }

    /// Set the read timeout for subsequent reads.
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        let _ = timeout;
        todo!("apply a new read timeout")
    }
}

impl ScpiTransport for RawTransport {
    fn send(&mut self, command: &str) -> Result<()> {
        let _ = command;
        todo!("write the command with a newline terminator")
    }

    fn receive(&mut self) -> Result<Vec<u8>> {
        todo!("read until newline, buffering any remainder")
    }

    fn close(&mut self) -> Result<()> {
        todo!("shut the socket down")
    }

    fn description(&self) -> String {
        let _ = self;
        todo!("describe the socket")
    }
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn test_raw_connect() {
        todo!("connect to the raw SCPI port")
    }

    #[test]
    #[ignore]
    fn test_raw_send() {
        todo!("write the command with a newline terminator")
    }

    #[test]
    #[ignore]
    fn test_raw_receive() {
        todo!("read until newline, buffering any remainder")
    }

    #[test]
    #[ignore]
    fn test_raw_partial_read() {
        todo!("handle partial reads")
    }

    #[test]
    #[ignore]
    fn test_raw_response_split_across_packets() {
        todo!("handle response split across packets")
    }

    #[test]
    #[ignore]
    fn test_raw_response_without_terminator_bounded() {
        todo!("response without terminator is bounded, not unbounded")
    }

    #[test]
    #[ignore]
    fn test_raw_bytes_past_terminator_buffered() {
        todo!("bytes read past the terminator are returned on the next receive")
    }

    #[test]
    #[ignore]
    fn test_raw_peer() {
        todo!("return the peer address")
    }

    #[test]
    #[ignore]
    fn test_raw_set_timeout() {
        todo!("apply a new read timeout")
    }
}
