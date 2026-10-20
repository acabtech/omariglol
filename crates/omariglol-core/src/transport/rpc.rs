//! Minimal ONC RPC (Sun RPC, RFC 1057) client with the XDR codec.
//!
//! LXI instruments speak VXI-11, which is an ONC RPC program. Rather than pull
//! in a heavyweight RPC stack we implement the small slice the instrument
//! actually needs:
//!
//! * `CALL`/`REPLY` framing over TCP with record marking
//! * `AUTH_NULL` credentials
//! * the portmapper's `GETPORT` and `DUMP` calls
//!
//! `DUMP` also has to run over **UDP broadcast** for discovery, which is the
//! main reason a general-purpose RPC library is not a good fit here.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use crate::error::Result;

pub const RPC_CALL: u32 = 0;
pub const RPC_REPLY: u32 = 1;
pub const RPC_VERSION: u32 = 2;

pub const AUTH_NULL: u32 = 0;

pub const MSG_ACCEPTED: u32 = 0;
pub const MSG_DENIED: u32 = 1;
pub const SUCCESS: u32 = 0;

/// The portmapper program, always at port 111.
pub const PROG_PORTMAP: u32 = 100_000;
pub const VERS_PORTMAP: u32 = 2;
pub const PROC_PMAP_GETPORT: u32 = 3;
pub const PROC_PMAP_DUMP: u32 = 4;

pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;

/// Largest record fragment we will reassemble, guarding against a hostile or
/// broken peer.
pub const MAX_RECORD: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// XDR
// ---------------------------------------------------------------------------

/// XDR encoder. All XDR quantities are 4-byte big-endian aligned.
#[derive(Default)]
pub struct Xdr {
    buf: Vec<u8>,
}

impl Xdr {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        let _ = v;
        todo!("append a big-endian u32")
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        let _ = v;
        todo!("append a big-endian i32")
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        let _ = v;
        todo!("append a boolean as u32")
    }

    /// Variable-length opaque, padded to a 4-byte boundary.
    pub fn opaque(&mut self, data: &[u8]) -> &mut Self {
        let _ = data;
        todo!("append length-prefixed, padded opaque data")
    }

    pub fn string(&mut self, s: &str) -> &mut Self {
        let _ = s;
        todo!("append a string as opaque data")
    }

    pub fn into_bytes(self) -> Vec<u8> {
        todo!("take the encoded buffer")
    }

    pub fn as_slice(&self) -> &[u8] {
        let _ = self;
        todo!("borrow the encoded buffer")
    }
}

/// XDR decoder over a fully-received message body.
pub struct XdrReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> XdrReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        let _ = buf;
        todo!("wrap a byte slice")
    }

    pub fn remaining(&self) -> usize {
        let _ = self;
        todo!("bytes left to decode")
    }

    pub fn u32(&mut self) -> Result<u32> {
        todo!("decode a big-endian u32")
    }

    pub fn i32(&mut self) -> Result<i32> {
        todo!("decode a big-endian i32")
    }

    pub fn bool(&mut self) -> Result<bool> {
        todo!("decode a boolean")
    }

    /// Variable-length opaque with padding skipped.
    pub fn opaque(&mut self) -> Result<&'a [u8]> {
        todo!("decode length-prefixed, padded opaque data")
    }

    pub fn string(&mut self) -> Result<String> {
        todo!("decode a string")
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        let _ = n;
        todo!("advance the cursor, bounds-checked")
    }
}

// ---------------------------------------------------------------------------
// RPC client
// ---------------------------------------------------------------------------

/// An ONC RPC client bound to one TCP connection.
pub struct RpcClient {
    stream: std::net::TcpStream,
}

impl RpcClient {
    /// Connect to an RPC service, applying `timeout` to connect and to all
    /// subsequent reads and writes.
    pub fn connect(addr: SocketAddr, timeout: Duration) -> Result<Self> {
        let _ = (addr, timeout);
        todo!("connect with a timeout and configure the socket")
    }

    /// Perform one RPC call and return the result body (everything after
    /// `accept_stat`). `params` must already be XDR-encoded.
    pub fn call(&mut self, prog: u32, vers: u32, proc: u32, params: &[u8]) -> Result<Vec<u8>> {
        let _ = (prog, vers, proc, params);
        todo!("frame, send, receive and unwrap an RPC reply")
    }
}

// ---------------------------------------------------------------------------
// Portmapper
// ---------------------------------------------------------------------------

/// One entry from the portmapper's program table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortMapping {
    pub prog: u32,
    pub vers: u32,
    pub prot: u32,
    pub port: u16,
}

/// Ask the portmapper on `addr:111` which port a program is registered on.
///
/// Returns `Ok(None)` when the program simply is not registered, which is the
/// normal answer for a host that is not an LXI instrument.
pub fn pmap_getport(
    addr: IpAddr,
    prog: u32,
    vers: u32,
    prot: u32,
    timeout: Duration,
) -> Result<Option<u16>> {
    let _ = (addr, prog, vers, prot, timeout);
    todo!("call PMAP_GETPORT")
}

/// Ask the portmapper for its full program table (`PMAP_DUMP`).
pub fn pmap_dump(addr: IpAddr, timeout: Duration) -> Result<Vec<PortMapping>> {
    let _ = (addr, timeout);
    todo!("call PMAP_DUMP and walk the mapping list")
}
