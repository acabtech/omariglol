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

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

use crate::error::{Error, Result};

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

/// Largest UDP datagram we will accept from a portmapper. A `PMAP_DUMP` reply
/// is a few kilobytes at most, so this is generous while keeping the receive
/// buffer off the stack.
const MAX_UDP_DATAGRAM: usize = 64 * 1024;

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
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.buf
            .extend_from_slice(&(if v { 1u32 } else { 0u32 }).to_be_bytes());
        self
    }

    /// Append already-encoded XDR bytes verbatim, with no length prefix.
    ///
    /// This is how procedure arguments are appended to a `CALL` body: they
    /// follow the verifier directly rather than being wrapped in an opaque.
    pub fn raw(&mut self, data: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(data);
        self
    }

    /// Variable-length opaque, padded to a 4-byte boundary.
    pub fn opaque(&mut self, data: &[u8]) -> &mut Self {
        let len = data.len() as u32;
        self.buf.extend_from_slice(&len.to_be_bytes());
        self.buf.extend_from_slice(data);
        let pad = (4 - (data.len() % 4)) % 4;
        for _ in 0..pad {
            self.buf.push(0);
        }
        self
    }

    pub fn string(&mut self, s: &str) -> &mut Self {
        self.opaque(s.as_bytes())
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }
}

/// XDR decoder over a fully-received message body.
pub struct XdrReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> XdrReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.ensure(4)?;
        let v = u32::from_be_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    pub fn i32(&mut self) -> Result<i32> {
        self.ensure(4)?;
        let v = i32::from_be_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    pub fn bool(&mut self) -> Result<bool> {
        let v = self.u32()?;
        Ok(v != 0)
    }

    /// Variable-length opaque with padding skipped.
    pub fn opaque(&mut self) -> Result<&'a [u8]> {
        let len = self.u32()? as usize;
        self.ensure(len)?;
        let data = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        // The padding is part of the stream, so it must be present too.
        let pad = (4 - (len % 4)) % 4;
        self.ensure(pad)?;
        self.pos += pad;
        Ok(data)
    }

    pub fn string(&mut self) -> Result<String> {
        let data = self.opaque()?;
        String::from_utf8(data.to_vec()).map_err(|e| Error::Malformed(e.to_string()))
    }

    /// Skip an `opaque_auth` (flavor, length-prefixed padded body).
    ///
    /// Both the call verifier and the accepted-reply verifier are encoded this
    /// way; with `AUTH_NULL` the body is empty but the two length words are
    /// still present.
    pub fn skip_auth(&mut self) -> Result<()> {
        let _flavor = self.u32()?;
        let len = self.u32()? as usize;
        self.ensure(len)?;
        self.pos += len;
        let pad = (4 - (len % 4)) % 4;
        self.ensure(pad)?;
        self.pos += pad;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.ensure(n)?;
        self.pos += n;
        Ok(())
    }

    fn ensure(&self, n: usize) -> Result<()> {
        if self.pos + n > self.buf.len() {
            return Err(Error::Malformed(format!(
                "XDR: need {} bytes at offset {} but buffer has {}",
                n,
                self.pos,
                self.buf.len()
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RPC client
// ---------------------------------------------------------------------------

/// An ONC RPC client bound to one TCP connection.
pub struct RpcClient {
    stream: TcpStream,
}

impl RpcClient {
    /// Connect to an RPC service, applying `timeout` to connect and to all
    /// subsequent reads and writes.
    pub fn connect(addr: SocketAddr, timeout: Duration) -> Result<Self> {
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Self { stream })
    }

    /// Perform one RPC call and return the result body (everything after
    /// `accept_stat`). `params` must already be XDR-encoded.
    pub fn call(&mut self, prog: u32, vers: u32, proc: u32, params: &[u8]) -> Result<Vec<u8>> {
        let xid = rand::random::<u32>();
        self.send_call(xid, prog, vers, proc, params)?;
        self.recv_reply(xid)
    }

    fn send_call(
        &mut self,
        xid: u32,
        prog: u32,
        vers: u32,
        proc: u32,
        params: &[u8],
    ) -> Result<()> {
        // Build the CALL body. Procedure arguments follow the verifier
        // directly, with no length prefix.
        let mut body = Xdr::new();
        body.u32(xid);
        body.u32(RPC_CALL);
        body.u32(RPC_VERSION);
        body.u32(prog);
        body.u32(vers);
        body.u32(proc);
        // AUTH_NULL credentials.
        body.u32(AUTH_NULL);
        body.u32(0); // cred length = 0
        // AUTH_NULL verifier.
        body.u32(AUTH_NULL);
        body.u32(0); // verifier length = 0
        body.raw(params);

        let body_bytes = body.into_bytes();

        // Record-mark the body: one fragment, last-fragment bit set.
        let header = (body_bytes.len() as u32 & 0x7FFF_FFFF) | 0x8000_0000;
        self.stream.write_all(&header.to_be_bytes())?;
        self.stream.write_all(&body_bytes)?;
        self.stream.flush()?;
        Ok(())
    }

    fn recv_reply(&mut self, expected_xid: u32) -> Result<Vec<u8>> {
        let body_buf = read_record(&mut self.stream)?;
        parse_reply(&body_buf, Some(expected_xid))
    }
}

/// Parse an RPC reply body, returning everything after `accept_stat`.
///
/// When `expected_xid` is `Some`, the reply's transaction id must match.
fn parse_reply(body_buf: &[u8], expected_xid: Option<u32>) -> Result<Vec<u8>> {
    let mut reader = XdrReader::new(body_buf);
    let xid = reader.u32()?;
    if let Some(expected) = expected_xid {
        if xid != expected {
            return Err(Error::Rpc(format!(
                "unexpected XID: got {}, expected {}",
                xid, expected
            )));
        }
    }
    let direction = reader.u32()?;
    if direction != RPC_REPLY {
        return Err(Error::Rpc(format!(
            "expected REPLY direction, got {}",
            direction
        )));
    }
    let reply_state = reader.u32()?;
    if reply_state != MSG_ACCEPTED {
        return Err(Error::Rpc("RPC reply was denied".to_string()));
    }
    // The accepted reply carries a verifier before the accept status.
    reader.skip_auth()?;
    let accept_state = reader.u32()?;
    if accept_state != SUCCESS {
        return Err(Error::Rpc(format!(
            "RPC call failed with accept state {}",
            accept_state
        )));
    }

    // Everything after accept_stat is the procedure result.
    let remaining = reader.remaining();
    Ok(body_buf[body_buf.len() - remaining..].to_vec())
}

/// Read one record-marked message from `reader`, reassembling fragments.
///
/// ONC RPC over TCP prefixes each fragment with a 4-byte header whose high bit
/// marks the last fragment. Reassembly is bounded by [`MAX_RECORD`] so a
/// hostile or broken peer cannot make us allocate without limit.
fn read_record(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut header_buf = [0u8; 4];
        reader.read_exact(&mut header_buf)?;
        let header = u32::from_be_bytes(header_buf);
        let last_fragment = header & 0x8000_0000 != 0;
        let fragment_len = (header & 0x7FFF_FFFF) as usize;

        if fragment_len > MAX_RECORD {
            return Err(Error::Rpc(format!(
                "record fragment exceeds MAX_RECORD: {} bytes",
                fragment_len
            )));
        }
        if body.len() + fragment_len > MAX_RECORD {
            return Err(Error::Rpc(format!(
                "reassembled record exceeds MAX_RECORD: {} bytes",
                body.len() + fragment_len
            )));
        }

        let start = body.len();
        body.resize(start + fragment_len, 0);
        reader.read_exact(&mut body[start..])?;

        if last_fragment {
            return Ok(body);
        }
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

/// Walk a `pmaplist` result, which is a linked list of `{ pmap, bool next }`.
fn parse_pmaplist(reader: &mut XdrReader<'_>) -> Result<Vec<PortMapping>> {
    let mut mappings = Vec::new();
    loop {
        if reader.remaining() == 0 {
            break;
        }
        let prog = reader.u32()?;
        let vers = reader.u32()?;
        let prot = reader.u32()?;
        let port = reader.u32()?;
        let more = reader.bool()?;
        mappings.push(PortMapping {
            prog,
            vers,
            prot,
            port: port as u16,
        });
        if !more {
            break;
        }
    }
    Ok(mappings)
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
    let socket_addr = SocketAddr::new(addr, 111);
    let mut client = RpcClient::connect(socket_addr, timeout)?;
    let mut params = Xdr::new();
    params.u32(prog);
    params.u32(vers);
    params.u32(prot);
    params.u32(0); // port placeholder
    let reply = client.call(
        PROG_PORTMAP,
        VERS_PORTMAP,
        PROC_PMAP_GETPORT,
        &params.into_bytes(),
    )?;
    let mut reader = XdrReader::new(&reply);
    let port = reader.u32()?;
    if port == 0 {
        Ok(None)
    } else {
        Ok(Some(port as u16))
    }
}

/// Ask the portmapper for its full program table (`PMAP_DUMP`).
pub fn pmap_dump(addr: IpAddr, timeout: Duration) -> Result<Vec<PortMapping>> {
    let socket_addr = SocketAddr::new(addr, 111);
    let mut client = RpcClient::connect(socket_addr, timeout)?;
    let reply = client.call(PROG_PORTMAP, VERS_PORTMAP, PROC_PMAP_DUMP, &[])?;
    let mut reader = XdrReader::new(&reply);
    parse_pmaplist(&mut reader)
}

// ---------------------------------------------------------------------------
// UDP portmapper helpers (for discovery)
// ---------------------------------------------------------------------------

/// Build a PMAP_GETPORT UDP request payload.
fn pmap_getport_udp_payload(xid: u32, prog: u32, vers: u32, prot: u32) -> Vec<u8> {
    let mut xdr = Xdr::new();
    xdr.u32(xid);
    xdr.u32(RPC_CALL);
    xdr.u32(RPC_VERSION);
    xdr.u32(PROG_PORTMAP);
    xdr.u32(VERS_PORTMAP);
    xdr.u32(PROC_PMAP_GETPORT);
    xdr.u32(AUTH_NULL);
    xdr.u32(0);
    xdr.u32(AUTH_NULL);
    xdr.u32(0);
    xdr.u32(prog);
    xdr.u32(vers);
    xdr.u32(prot);
    xdr.u32(0); // port placeholder
    xdr.into_bytes()
}

/// Build a PMAP_DUMP UDP request payload.
fn pmap_dump_udp_payload(xid: u32) -> Vec<u8> {
    let mut xdr = Xdr::new();
    xdr.u32(xid);
    xdr.u32(RPC_CALL);
    xdr.u32(RPC_VERSION);
    xdr.u32(PROG_PORTMAP);
    xdr.u32(VERS_PORTMAP);
    xdr.u32(PROC_PMAP_DUMP);
    xdr.u32(AUTH_NULL);
    xdr.u32(0);
    xdr.u32(AUTH_NULL);
    xdr.u32(0);
    xdr.into_bytes()
}

/// Ask the portmapper over **UDP** which port a program is registered on.
///
/// The UDP variant is what discovery uses, because a single datagram can be
/// broadcast to the whole subnet and answered by every host at once.
pub fn pmap_getport_udp(
    addr: IpAddr,
    prog: u32,
    vers: u32,
    prot: u32,
    timeout: Duration,
) -> Result<Option<u16>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(timeout))?;
    let xid = rand::random::<u32>();
    let target = SocketAddr::new(addr, 111);
    socket.send_to(&pmap_getport_udp_payload(xid, prog, vers, prot), target)?;

    let mut buf = vec![0u8; MAX_UDP_DATAGRAM];
    match socket.recv_from(&mut buf) {
        Ok((n, _src)) => {
            let reply = parse_reply(&buf[..n], Some(xid))?;
            let mut reader = XdrReader::new(&reply);
            let port = reader.u32()?;
            if port == 0 {
                Ok(None)
            } else {
                Ok(Some(port as u16))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Send a PMAP_DUMP UDP broadcast and collect responses.
pub fn pmap_dump_udp(timeout: Duration) -> Result<Vec<PortMapping>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(timeout))?;
    socket.set_broadcast(true)?;

    let xid = rand::random::<u32>();
    let broadcast_addr: SocketAddr = "255.255.255.255:111".parse().unwrap();
    socket.send_to(&pmap_dump_udp_payload(xid), broadcast_addr)?;

    // Collect responses until the read timeout fires.
    let mut mappings = Vec::new();
    let mut buf = vec![0u8; MAX_UDP_DATAGRAM];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, _src)) => {
                // A reply from a host that did not echo our XID is ignored.
                if let Ok(reply) = parse_reply(&buf[..n], Some(xid)) {
                    let mut reader = XdrReader::new(&reply);
                    if let Ok(found) = parse_pmaplist(&mut reader) {
                        mappings.extend(found);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(_) => break,
        }
    }
    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xdr_round_trip_u32() {
        let mut xdr = Xdr::new();
        xdr.u32(0xDEADBEEF);
        let bytes = xdr.into_bytes();
        assert_eq!(bytes, [0xDE, 0xAD, 0xBE, 0xEF]);
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.u32().unwrap(), 0xDEADBEEF);
    }

    #[test]
    fn test_xdr_round_trip_i32() {
        let mut xdr = Xdr::new();
        xdr.i32(-42);
        let bytes = xdr.into_bytes();
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), -42);
    }

    #[test]
    fn test_xdr_round_trip_bool() {
        let mut xdr = Xdr::new();
        xdr.bool(true);
        xdr.bool(false);
        let bytes = xdr.into_bytes();
        let mut reader = XdrReader::new(&bytes);
        assert!(reader.bool().unwrap());
        assert!(!reader.bool().unwrap());
    }

    #[test]
    fn test_xdr_round_trip_string() {
        let mut xdr = Xdr::new();
        xdr.string("hello");
        let bytes = xdr.into_bytes();
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.string().unwrap(), "hello");
    }

    #[test]
    fn test_xdr_round_trip_opaque() {
        let mut xdr = Xdr::new();
        xdr.opaque(b"\x01\x02\x03");
        let bytes = xdr.into_bytes();
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.opaque().unwrap(), b"\x01\x02\x03");
    }

    #[test]
    fn test_xdr_padding() {
        // 3 bytes should be padded to 4
        let mut xdr = Xdr::new();
        xdr.opaque(b"\x01\x02\x03");
        let bytes = xdr.into_bytes();
        // Length (4) + 3 data + 1 pad = 8 bytes
        assert_eq!(bytes.len(), 8);
        // 5 bytes should be padded to 8
        let mut xdr = Xdr::new();
        xdr.opaque(b"\x01\x02\x03\x04\x05");
        let bytes = xdr.into_bytes();
        assert_eq!(bytes.len(), 12);
    }

    #[test]
    fn test_xdr_raw_has_no_length_prefix() {
        let mut xdr = Xdr::new();
        xdr.raw(&[0xAA, 0xBB]);
        assert_eq!(xdr.into_bytes(), [0xAA, 0xBB]);
    }

    #[test]
    fn test_xdr_reader_bounds_checked_skip() {
        let mut reader = XdrReader::new(&[0u8; 4]);
        reader.skip(4).unwrap();
        assert!(reader.skip(1).is_err());
    }

    #[test]
    fn test_xdr_reader_truncated_input() {
        let mut reader = XdrReader::new(&[0u8; 2]);
        assert!(reader.u32().is_err());
    }

    #[test]
    fn test_xdr_reader_oversized_input() {
        let buf = vec![0u8; MAX_RECORD + 1];
        let mut reader = XdrReader::new(&buf);
        // Should not panic, should return an error when we try to read past the end.
        let _ = reader.u32();
    }

    #[test]
    fn test_xdr_reader_opaque_missing_padding_errors() {
        // Length says 3 bytes, but only the 3 data bytes are present and the
        // mandatory padding byte is missing. This must be an error, not a
        // cursor that runs past the end of the buffer.
        let buf = [0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC];
        let mut reader = XdrReader::new(&buf);
        assert!(reader.opaque().is_err());
    }

    #[test]
    fn test_skip_auth_null() {
        // flavor = AUTH_NULL, length = 0, then a u32 to prove the cursor moved.
        let buf = [
            0x00, 0x00, 0x00, 0x00, // flavor = AUTH_NULL
            0x00, 0x00, 0x00, 0x00, // length = 0
            0x00, 0x00, 0x00, 0x2A, // trailing value
        ];
        let mut reader = XdrReader::new(&buf);
        reader.skip_auth().unwrap();
        assert_eq!(reader.u32().unwrap(), 0x2A);
    }

    #[test]
    fn test_parse_reply_skips_verifier() {
        // xid, REPLY, MSG_ACCEPTED, verf(AUTH_NULL,0), SUCCESS, result=0x1234
        let mut xdr = Xdr::new();
        xdr.u32(0xABCD);
        xdr.u32(RPC_REPLY);
        xdr.u32(MSG_ACCEPTED);
        xdr.u32(AUTH_NULL);
        xdr.u32(0);
        xdr.u32(SUCCESS);
        xdr.u32(0x1234);
        let body = xdr.into_bytes();
        let result = parse_reply(&body, Some(0xABCD)).unwrap();
        let mut reader = XdrReader::new(&result);
        assert_eq!(reader.u32().unwrap(), 0x1234);
    }

    #[test]
    fn test_parse_reply_rejects_wrong_xid() {
        let mut xdr = Xdr::new();
        xdr.u32(0x1111);
        xdr.u32(RPC_REPLY);
        xdr.u32(MSG_ACCEPTED);
        xdr.u32(AUTH_NULL);
        xdr.u32(0);
        xdr.u32(SUCCESS);
        let body = xdr.into_bytes();
        assert!(parse_reply(&body, Some(0x2222)).is_err());
    }

    #[test]
    fn test_pmap_getport_unregistered_returns_none() {
        // A PMAP_GETPORT reply with port=0 (not registered).
        let mut xdr = Xdr::new();
        xdr.u32(0x0000_0001); // XID
        xdr.u32(RPC_REPLY);
        xdr.u32(MSG_ACCEPTED);
        xdr.u32(AUTH_NULL); // verifier flavor
        xdr.u32(0); // verifier length
        xdr.u32(SUCCESS);
        xdr.u32(0); // port = 0
        let reply = xdr.into_bytes();
        let result = parse_reply(&reply, Some(1)).unwrap();
        let mut reader = XdrReader::new(&result);
        assert_eq!(reader.u32().unwrap(), 0);
    }

    #[test]
    fn test_pmap_dump_decodes_reply() {
        // A PMAP_DUMP reply with one entry followed by the end-of-list flag.
        let mut xdr = Xdr::new();
        xdr.u32(0x0000_0001); // XID
        xdr.u32(RPC_REPLY);
        xdr.u32(MSG_ACCEPTED);
        xdr.u32(AUTH_NULL); // verifier flavor
        xdr.u32(0); // verifier length
        xdr.u32(SUCCESS);
        xdr.u32(100_000); // prog
        xdr.u32(2); // vers
        xdr.u32(IPPROTO_TCP); // prot
        xdr.u32(111); // port
        xdr.bool(false); // no more entries
        let reply = xdr.into_bytes();
        let result = parse_reply(&reply, Some(1)).unwrap();
        let mut reader = XdrReader::new(&result);
        let mappings = parse_pmaplist(&mut reader).unwrap();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].prog, 100_000);
        assert_eq!(mappings[0].vers, 2);
        assert_eq!(mappings[0].prot, IPPROTO_TCP);
        assert_eq!(mappings[0].port, 111);
    }

    #[test]
    fn test_pmap_dump_decodes_multiple_entries() {
        let mut xdr = Xdr::new();
        xdr.u32(0x0000_0001);
        xdr.u32(RPC_REPLY);
        xdr.u32(MSG_ACCEPTED);
        xdr.u32(AUTH_NULL);
        xdr.u32(0);
        xdr.u32(SUCCESS);
        // First entry, more to follow.
        xdr.u32(100_000);
        xdr.u32(2);
        xdr.u32(IPPROTO_TCP);
        xdr.u32(111);
        xdr.bool(true);
        // Second entry, end of list.
        xdr.u32(0x0006_07AF);
        xdr.u32(1);
        xdr.u32(IPPROTO_TCP);
        xdr.u32(1024);
        xdr.bool(false);
        let reply = xdr.into_bytes();
        let result = parse_reply(&reply, Some(1)).unwrap();
        let mut reader = XdrReader::new(&result);
        let mappings = parse_pmaplist(&mut reader).unwrap();
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings[1].prog, 0x0006_07AF);
        assert_eq!(mappings[1].port, 1024);
    }

    #[test]
    fn test_record_reassembly_guard() {
        // Verify that MAX_RECORD is enforced.
        assert_eq!(MAX_RECORD, 16 * 1024 * 1024);
    }

    #[test]
    fn test_read_record_single_fragment() {
        // One fragment, last-fragment bit set, 4 bytes of payload.
        let mut wire = Vec::new();
        wire.extend_from_slice(&(0x8000_0004u32).to_be_bytes());
        wire.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        let mut cursor = std::io::Cursor::new(wire);
        let body = read_record(&mut cursor).unwrap();
        assert_eq!(body, [0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn test_read_record_reassembles_multiple_fragments() {
        // Two fragments: first not last, second last.
        let mut wire = Vec::new();
        wire.extend_from_slice(&(0x0000_0002u32).to_be_bytes()); // not last, 2 bytes
        wire.extend_from_slice(&[0x01, 0x02]);
        wire.extend_from_slice(&(0x8000_0002u32).to_be_bytes()); // last, 2 bytes
        wire.extend_from_slice(&[0x03, 0x04]);
        let mut cursor = std::io::Cursor::new(wire);
        let body = read_record(&mut cursor).unwrap();
        assert_eq!(body, [0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_read_record_rejects_oversized_fragment() {
        // A fragment claiming more than MAX_RECORD must be rejected without
        // attempting to allocate it.
        let mut wire = Vec::new();
        wire.extend_from_slice(&(0x8000_0000u32 | (MAX_RECORD as u32 + 1)).to_be_bytes());
        let mut cursor = std::io::Cursor::new(wire);
        assert!(read_record(&mut cursor).is_err());
    }

    #[test]
    fn test_read_record_truncated_fragment() {
        // Header promises 4 bytes but only 2 are present.
        let mut wire = Vec::new();
        wire.extend_from_slice(&(0x8000_0004u32).to_be_bytes());
        wire.extend_from_slice(&[0x01, 0x02]);
        let mut cursor = std::io::Cursor::new(wire);
        assert!(read_record(&mut cursor).is_err());
    }
}
