//! Raw TCP SCPI transport (the `lxi scpi --raw` path).
//!
//! Instruments supporting the LXI "raw socket" convention listen on TCP 5025
//! and speak bare SCPI terminated by `\n`, with no RPC framing at all.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

use super::ScpiTransport;
use crate::device::Target;
use crate::error::{Error, Result};

/// The IANA-assigned LXI raw socket port.
pub const DEFAULT_RAW_PORT: u16 = 5025;

/// Upper bound on a single raw response, so an instrument that never sends a
/// terminator cannot exhaust memory.
pub const MAX_RESPONSE: usize = 32 * 1024 * 1024;

/// The SCPI terminator.
const TERMINATOR: u8 = b'\n';

/// Total length of a definite-length IEEE-488.2 block (`#<n><len><data>`) in
/// `data`, if the header is fully present.
///
/// Returns `None` for text, indefinite blocks (`#0`) and incomplete headers,
/// which fall back to newline framing. Mirrors the parser in
/// [`crate::screenshot`] but is kept local so the transport layer stays
/// independent of the screenshot layer.
fn declared_block_len(data: &[u8]) -> Option<usize> {
    if data.first() != Some(&b'#') {
        return None;
    }
    let digit = *data.get(1)?;
    if !digit.is_ascii_digit() {
        return None;
    }
    let n = (digit - b'0') as usize;
    if n == 0 {
        return None;
    }
    let len_start: usize = 2;
    let len_end = len_start.checked_add(n)?;
    let digits = data.get(len_start..len_end)?;
    let len: usize = std::str::from_utf8(digits).ok()?.parse().ok()?;
    len_end.checked_add(len)
}

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
        let port = target.port.unwrap_or(DEFAULT_RAW_PORT);
        let addr = SocketAddr::new(target.address, port);
        let stream = TcpStream::connect_timeout(&addr, target.timeout)?;
        stream.set_read_timeout(Some(target.timeout))?;
        stream.set_write_timeout(Some(target.timeout))?;
        Ok(Self {
            stream,
            timeout: target.timeout,
            pending: Vec::new(),
            addr,
        })
    }

    /// The socket address this transport is connected to.
    pub fn peer(&self) -> SocketAddr {
        self.addr
    }

    /// Set the read timeout for subsequent reads.
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.stream.set_read_timeout(Some(timeout))?;
        self.timeout = timeout;
        Ok(())
    }

    /// Pull one newline-terminated line out of `pending`, if one is buffered.
    ///
    /// The terminator (and a preceding `\r`, for CRLF instruments) is stripped.
    fn take_line(&mut self) -> Option<Vec<u8>> {
        let pos = self.pending.iter().position(|&b| b == TERMINATOR)?;
        let mut line: Vec<u8> = self.pending.drain(..=pos).collect();
        line.pop(); // drop the '\n'
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        Some(line)
    }
}

impl ScpiTransport for RawTransport {
    fn send(&mut self, command: &str) -> Result<()> {
        let mut data = command.as_bytes().to_vec();
        if data.last() != Some(&TERMINATOR) {
            data.push(TERMINATOR);
        }
        self.stream.write_all(&data)?;
        self.stream.flush()?;
        Ok(())
    }

    fn receive(&mut self) -> Result<Vec<u8>> {
        let mut buf = [0u8; 4096];
        loop {
            // A definite-length IEEE-488.2 block declares its own size, so read
            // exactly that many bytes rather than waiting for a newline that a
            // binary payload may never contain. This is what makes large binary
            // reads — screenshots — both correct and fast over the raw socket:
            // the whole image streams in at TCP speed instead of being truncated
            // at the first `\n` byte inside the pixel data.
            if let Some(total) = declared_block_len(&self.pending) {
                while self.pending.len() < total {
                    let n = self.stream.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    self.pending.extend_from_slice(&buf[..n]);
                    if self.pending.len() > MAX_RESPONSE {
                        return Err(Error::Malformed(format!(
                            "raw response exceeds MAX_RESPONSE ({} bytes)",
                            MAX_RESPONSE
                        )));
                    }
                }
                let take = total.min(self.pending.len());
                return Ok(self.pending.drain(..take).collect());
            }

            // A complete newline-terminated line already buffered?
            if let Some(line) = self.take_line() {
                return Ok(line);
            }

            let n = self.stream.read(&mut buf)?;
            if n == 0 {
                // The peer closed. Hand back whatever is buffered, or report
                // EOF when there is nothing to return.
                if self.pending.is_empty() {
                    return Err(Error::Io(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "raw socket closed before a response was received",
                    )));
                }
                let mut line = std::mem::take(&mut self.pending);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(line);
            }

            self.pending.extend_from_slice(&buf[..n]);
            if self.pending.len() > MAX_RESPONSE {
                return Err(Error::Malformed(format!(
                    "raw response exceeds MAX_RESPONSE ({} bytes) without a terminator",
                    MAX_RESPONSE
                )));
            }
        }
    }

    fn close(&mut self) -> Result<()> {
        // Best effort: the peer may already be gone.
        let _ = self.stream.shutdown(Shutdown::Both);
        Ok(())
    }

    fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        RawTransport::set_timeout(self, timeout)
    }

    fn description(&self) -> String {
        format!("raw TCP {}", self.addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Transport;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    /// Start a one-shot TCP server on an ephemeral port and run `handler`
    /// against the first accepted connection.
    fn spawn_server<F>(handler: F) -> (SocketAddr, thread::JoinHandle<()>)
    where
        F: FnOnce(TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                handler(stream);
            }
        });
        (addr, handle)
    }

    fn connect_to(addr: SocketAddr) -> RawTransport {
        let target = Target {
            address: addr.ip(),
            transport: Transport::Raw,
            port: Some(addr.port()),
            device_name: "inst0".to_string(),
            timeout: Duration::from_secs(2),
        };
        RawTransport::connect(&target).unwrap()
    }

    #[test]
    fn test_raw_connect() {
        let (addr, handle) = spawn_server(|_s| {});
        let transport = connect_to(addr);
        assert_eq!(transport.peer(), addr);
        let _ = handle.join();
    }

    #[test]
    fn test_raw_send() {
        let (tx, rx) = mpsc::channel();
        let (addr, handle) = spawn_server(move |mut s| {
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            while let Ok(n) = s.read(&mut byte) {
                if n == 0 || byte[0] == TERMINATOR {
                    break;
                }
                buf.push(byte[0]);
            }
            let _ = tx.send(buf);
        });
        let mut transport = connect_to(addr);
        transport.send("*IDN?").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), b"*IDN?");
        let _ = handle.join();
    }

    #[test]
    fn test_raw_receive() {
        let (addr, handle) = spawn_server(|mut s| {
            let _ = s.write_all(b"hello\n");
            let _ = s.flush();
            thread::sleep(Duration::from_millis(200));
        });
        let mut transport = connect_to(addr);
        assert_eq!(transport.receive().unwrap(), b"hello");
        let _ = handle.join();
    }

    #[test]
    fn test_raw_partial_read() {
        let (addr, handle) = spawn_server(|mut s| {
            let _ = s.write_all(b"par");
            let _ = s.flush();
            thread::sleep(Duration::from_millis(50));
            let _ = s.write_all(b"tial\n");
            let _ = s.flush();
            thread::sleep(Duration::from_millis(200));
        });
        let mut transport = connect_to(addr);
        assert_eq!(transport.receive().unwrap(), b"partial");
        let _ = handle.join();
    }

    #[test]
    fn test_raw_response_split_across_packets() {
        let (addr, handle) = spawn_server(|mut s| {
            for chunk in [&b"one "[..], &b"two "[..], &b"three\n"[..]] {
                let _ = s.write_all(chunk);
                let _ = s.flush();
                thread::sleep(Duration::from_millis(30));
            }
            thread::sleep(Duration::from_millis(200));
        });
        let mut transport = connect_to(addr);
        assert_eq!(transport.receive().unwrap(), b"one two three");
        let _ = handle.join();
    }

    #[test]
    fn test_raw_response_without_terminator_bounded() {
        let (addr, handle) = spawn_server(|mut s| {
            let _ = s.write_all(b"x");
            let _ = s.flush();
            thread::sleep(Duration::from_millis(200));
        });
        let mut transport = connect_to(addr);
        // Pretend a full response with no terminator is already buffered; the
        // next byte must trip the MAX_RESPONSE guard rather than grow forever.
        transport.pending = vec![b'a'; MAX_RESPONSE];
        let err = transport.receive().unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
        let _ = handle.join();
    }

    #[test]
    fn test_raw_receive_definite_block_with_newline_in_payload() {
        // A definite-length block whose binary payload contains a `\n` must be
        // read in full, not truncated at the newline. The whole block (header
        // included) is returned; the screenshot layer strips the header.
        let payload: &[u8] = b"BM\x00\x0a\x00binary\x0a\x00";
        let mut block = format!("#2{:02}", payload.len()).into_bytes();
        block.extend_from_slice(payload);
        let expected = block.clone();
        let (addr, handle) = spawn_server(move |mut s| {
            let _ = s.write_all(&block);
            let _ = s.flush();
            thread::sleep(Duration::from_millis(200));
        });
        let mut transport = connect_to(addr);
        assert_eq!(transport.receive().unwrap(), expected);
        let _ = handle.join();
    }

    #[test]
    fn test_raw_receive_block_split_across_packets() {
        // The block header and payload may arrive in separate TCP segments.
        let payload: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x01\x02\x03";
        let mut block = format!("#2{:02}", payload.len()).into_bytes();
        block.extend_from_slice(payload);
        let expected = block.clone();
        let (head, tail) = block.split_at(5);
        let head = head.to_vec();
        let tail = tail.to_vec();
        let (addr, handle) = spawn_server(move |mut s| {
            let _ = s.write_all(&head);
            let _ = s.flush();
            thread::sleep(Duration::from_millis(50));
            let _ = s.write_all(&tail);
            let _ = s.flush();
            thread::sleep(Duration::from_millis(200));
        });
        let mut transport = connect_to(addr);
        assert_eq!(transport.receive().unwrap(), expected);
        let _ = handle.join();
    }

    #[test]
    fn test_raw_bytes_past_terminator_buffered() {
        let (addr, handle) = spawn_server(|mut s| {
            let _ = s.write_all(b"first\nsecond\n");
            let _ = s.flush();
            thread::sleep(Duration::from_millis(200));
        });
        let mut transport = connect_to(addr);
        assert_eq!(transport.receive().unwrap(), b"first");
        assert_eq!(transport.receive().unwrap(), b"second");
        let _ = handle.join();
    }

    #[test]
    fn test_raw_peer() {
        let (addr, handle) = spawn_server(|_s| {});
        let transport = connect_to(addr);
        assert_eq!(transport.peer(), addr);
        let _ = handle.join();
    }

    #[test]
    fn test_raw_set_timeout() {
        let (addr, handle) = spawn_server(|_s| {});
        let mut transport = connect_to(addr);
        transport.set_timeout(Duration::from_millis(123)).unwrap();
        assert_eq!(transport.timeout, Duration::from_millis(123));
        let _ = handle.join();
    }
}
