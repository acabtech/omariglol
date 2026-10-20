//! Transport abstraction shared by the CLI, the GUI and the shell plugin.

pub mod raw;
pub mod rpc;
pub mod vxi11;

use std::net::{IpAddr, ToSocketAddrs};

use crate::device::{Target, Transport};
use crate::error::{Error, Result};

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
        self.send(command)?;
        self.receive()
    }

    /// Release the link. Called on drop paths where possible.
    fn close(&mut self) -> Result<()>;

    /// Human-readable description of the underlying channel.
    fn description(&self) -> String;
}

/// Open a transport for `target`.
pub fn open(target: &Target) -> Result<Box<dyn ScpiTransport>> {
    match target.transport {
        Transport::Vxi11 => Ok(Box::new(vxi11::Vxi11Transport::connect(target)?)),
        Transport::Raw => Ok(Box::new(raw::RawTransport::connect(target)?)),
    }
}

/// A convenient wrapper that owns a transport and tracks the target, with
/// helpers for the text-oriented operations the UI needs.
pub struct Session {
    transport: Box<dyn ScpiTransport>,
    pub target: Target,
}

impl Session {
    pub fn open(target: Target) -> Result<Self> {
        let transport = open(&target)?;
        Ok(Self { transport, target })
    }

    /// Send a command and discard any response.
    pub fn send(&mut self, command: &str) -> Result<()> {
        self.transport.send(command)
    }

    /// Send a command and return the raw response bytes.
    pub fn query_raw(&mut self, command: &str) -> Result<Vec<u8>> {
        self.transport.query(command)
    }

    /// Send a query and decode the response as a trimmed string, as
    /// `lxi scpi` prints it.
    pub fn query(&mut self, command: &str) -> Result<String> {
        let bytes = self.transport.query(command)?;
        Ok(String::from_utf8_lossy(&bytes).trim().to_string())
    }

    /// Identify the instrument with `*IDN?`.
    pub fn identify(&mut self) -> Result<String> {
        self.query("*IDN?")
    }

    pub fn description(&self) -> String {
        self.transport.description()
    }

    pub fn close(&mut self) -> Result<()> {
        self.transport.close()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Best effort; a failure here is not actionable for the caller.
        let _ = self.transport.close();
    }
}

/// Format raw response bytes the way `lxi scpi --hex` does.
pub fn to_hex(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 3);
    for (i, byte) in data.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Interpret an instrument address string ("192.168.1.10", "scope.local").
///
/// A literal IP is parsed directly; anything else is resolved as a hostname,
/// taking the first address the resolver returns.
pub fn parse_address(input: &str) -> Result<IpAddr> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(Error::Addr("empty address".to_string()));
    }
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Ok(ip);
    }
    let mut addrs = (trimmed, 0u16)
        .to_socket_addrs()
        .map_err(|e| Error::Addr(format!("{trimmed}: {e}")))?;
    addrs
        .next()
        .map(|sa| sa.ip())
        .ok_or_else(|| Error::Addr(format!("no address found for {trimmed}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    /// Shared state so a test can inspect a transport after it has been boxed
    /// into a [`Session`].
    #[derive(Default)]
    struct MockState {
        sent: Vec<String>,
        responses: VecDeque<Vec<u8>>,
        closed: bool,
    }

    struct MockTransport(Arc<Mutex<MockState>>);

    impl MockTransport {
        fn new(responses: Vec<&[u8]>) -> (Self, Arc<Mutex<MockState>>) {
            let state = Arc::new(Mutex::new(MockState {
                responses: responses.into_iter().map(|r| r.to_vec()).collect(),
                ..Default::default()
            }));
            (Self(state.clone()), state)
        }
    }

    impl ScpiTransport for MockTransport {
        fn send(&mut self, command: &str) -> Result<()> {
            self.0.lock().unwrap().sent.push(command.to_string());
            Ok(())
        }

        fn receive(&mut self) -> Result<Vec<u8>> {
            self.0
                .lock()
                .unwrap()
                .responses
                .pop_front()
                .ok_or_else(|| Error::Other("mock has no response queued".to_string()))
        }

        fn close(&mut self) -> Result<()> {
            self.0.lock().unwrap().closed = true;
            Ok(())
        }

        fn description(&self) -> String {
            "mock transport".to_string()
        }
    }

    fn mock_session(responses: Vec<&[u8]>) -> (Session, Arc<Mutex<MockState>>) {
        let (transport, state) = MockTransport::new(responses);
        let target = Target::new("127.0.0.1".parse().unwrap(), Transport::Raw);
        (
            Session {
                transport: Box::new(transport),
                target,
            },
            state,
        )
    }

    /// A one-shot TCP server on an ephemeral port, for exercising `open`.
    fn spawn_server() -> (std::net::SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let _ = listener.accept();
            thread::sleep(Duration::from_millis(100));
        });
        (addr, handle)
    }

    #[test]
    fn test_scpi_transport_query() {
        let (mut transport, state) = MockTransport::new(vec![b"42"]);
        let response = transport.query("*IDN?").unwrap();
        assert_eq!(response, b"42");
        assert_eq!(state.lock().unwrap().sent, vec!["*IDN?".to_string()]);
    }

    #[test]
    fn test_open_dispatch() {
        let (addr, handle) = spawn_server();
        let target = Target::new(addr.ip(), Transport::Raw)
            .with_port(addr.port())
            .with_timeout(Duration::from_secs(2));
        let transport = open(&target).unwrap();
        assert!(transport.description().starts_with("raw TCP"));
        let _ = handle.join();
    }

    #[test]
    fn test_session_open() {
        let (addr, handle) = spawn_server();
        let target = Target::new(addr.ip(), Transport::Raw)
            .with_port(addr.port())
            .with_timeout(Duration::from_secs(2));
        let session = Session::open(target).unwrap();
        assert!(session.description().starts_with("raw TCP"));
        let _ = handle.join();
    }

    #[test]
    fn test_session_send() {
        let (mut session, state) = mock_session(vec![]);
        session.send("*CLS").unwrap();
        assert_eq!(state.lock().unwrap().sent, vec!["*CLS".to_string()]);
    }

    #[test]
    fn test_session_query_raw() {
        let (mut session, _state) = mock_session(vec![b"raw bytes"]);
        assert_eq!(session.query_raw("*IDN?").unwrap(), b"raw bytes");
    }

    #[test]
    fn test_session_query() {
        let (mut session, _state) = mock_session(vec![b"  hello world  "]);
        assert_eq!(session.query(":MEAS?").unwrap(), "hello world");
    }

    #[test]
    fn test_session_identify() {
        let (mut session, state) = mock_session(vec![b"RIGOL TECHNOLOGIES,DS1054Z\n"]);
        assert_eq!(session.identify().unwrap(), "RIGOL TECHNOLOGIES,DS1054Z");
        assert_eq!(state.lock().unwrap().sent, vec!["*IDN?".to_string()]);
    }

    #[test]
    fn test_session_description() {
        let (session, _state) = mock_session(vec![]);
        assert_eq!(session.description(), "mock transport");
    }

    #[test]
    fn test_session_close() {
        let (mut session, state) = mock_session(vec![]);
        session.close().unwrap();
        assert!(state.lock().unwrap().closed);
    }

    #[test]
    fn test_session_drop_closes() {
        let state = {
            let (session, state) = mock_session(vec![]);
            drop(session);
            state
        };
        assert!(state.lock().unwrap().closed);
    }

    #[test]
    fn test_to_hex() {
        assert_eq!(to_hex(&[]), "");
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "00 0f ff");
        assert_eq!(to_hex(b"AB"), "41 42");
    }

    #[test]
    fn test_parse_address_ip() {
        assert_eq!(
            parse_address("192.168.1.10").unwrap(),
            "192.168.1.10".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            parse_address("  10.0.0.1  ").unwrap(),
            "10.0.0.1".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn test_parse_address_hostname() {
        let ip = parse_address("localhost").unwrap();
        assert!(
            ip.is_loopback(),
            "localhost should resolve to a loopback address"
        );
    }

    #[test]
    fn test_parse_address_empty() {
        assert!(matches!(parse_address("   "), Err(Error::Addr(_))));
    }
}
