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
        let pad = (4 - (len % 4)) % 4;
        self.pos += pad;
        Ok(data)
    }

    pub fn string(&mut self) -> Result<String> {
        let data = self.opaque()?;
        String::from_utf8(data.to_vec()).map_err(|e| Error::Malformed(e.to_string()))
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
        // Build the CALL body.
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
        body.opaque(params);

        let body_bytes = body.into_bytes();

        // Record-mark the body.
        let last = true;
        let mut header = (body_bytes.len() as u32) & 0x7FFF_FFFF;
        if last {
            header |= 0x8000_0000;
        }
        self.stream.write_all(&header.to_be_bytes())?;
        self.stream.write_all(&body_bytes)?;
        self.stream.flush()?;
        Ok(())
    }

    fn recv_reply(&mut self, expected_xid: u32) -> Result<Vec<u8>> {
        let body_buf = read_record(&mut self.stream)?;

        // Parse the reply.
        let mut reader = XdrReader::new(&body_buf);
        let xid = reader.u32()?;
        if xid != expected_xid {
            return Err(Error::Rpc(format!(
                "unexpected XID: got {}, expected {}",
                xid, expected_xid
            )));
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
        let accept_state = reader.u32()?;
        if accept_state != SUCCESS {
            return Err(Error::Rpc(format!(
                "RPC call failed with accept state {}",
                accept_state
            )));
        }

        // Return everything after accept_stat (the result body).
        let remaining = reader.remaining();
        Ok(body_buf[body_buf.len() - remaining..].to_vec())
    }
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
    let port = match addr {
        IpAddr::V4(a) => a,
        IpAddr::V6(_) => return Err(Error::Addr("IPv6 not supported for portmapper".to_string())),
    };
    let socket_addr = SocketAddr::new(IpAddr::V4(port), 111);
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
    let port = match addr {
        IpAddr::V4(a) => a,
        IpAddr::V6(_) => return Err(Error::Addr("IPv6 not supported for portmapper".to_string())),
    };
    let socket_addr = SocketAddr::new(IpAddr::V4(port), 111);
    let mut client = RpcClient::connect(socket_addr, timeout)?;
    let reply = client.call(PROG_PORTMAP, VERS_PORTMAP, PROC_PMAP_DUMP, &[])?;
    let mut reader = XdrReader::new(&reply);
    let mut mappings = Vec::new();
    loop {
        let remaining = reader.remaining();
        if remaining == 0 {
            break;
        }
        let prog = reader.u32()?;
        let vers = reader.u32()?;
        let prot = reader.u32()?;
        let port = reader.u32()?;
        mappings.push(PortMapping {
            prog,
            vers,
            prot,
            port: port as u16,
        });
    }
    Ok(mappings)
}

// ---------------------------------------------------------------------------
// UDP portmapper helpers (for discovery)
// ---------------------------------------------------------------------------

/// Build a PMAP_GETPORT UDP request payload.
fn pmap_getport_udp_payload(prog: u32, vers: u32, prot: u32) -> Vec<u8> {
    let mut xdr = Xdr::new();
    xdr.u32(0); // XID placeholder
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

/// Parse a PMAP_GETPORT UDP reply.
fn parse_pmap_getport_udp_reply(buf: &[u8]) -> Result<Option<u16>> {
    // UDP RPC replies have a 4-byte XID, then the same body as TCP.
    if buf.len() < 8 {
        return Err(Error::Malformed(
            "PMAP_GETPORT UDP reply too short".to_string(),
        ));
    }
    let mut reader = XdrReader::new(&buf[4..]); // skip XID
    let direction = reader.u32()?;
    if direction != RPC_REPLY {
        return Err(Error::Rpc(
            "expected REPLY in UDP pmap response".to_string(),
        ));
    }
    let reply_state = reader.u32()?;
    if reply_state != MSG_ACCEPTED {
        return Err(Error::Rpc("PMAP_GETPORT UDP reply denied".to_string()));
    }
    let accept_state = reader.u32()?;
    if accept_state != SUCCESS {
        return Err(Error::Rpc("PMAP_GETPORT UDP call failed".to_string()));
    }
    let port = reader.u32()?;
    if port == 0 {
        Ok(None)
    } else {
        Ok(Some(port as u16))
    }
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
    let target = SocketAddr::new(addr, 111);
    socket.send_to(&pmap_getport_udp_payload(prog, vers, prot), target)?;

    let mut buf = [0u8; 1024];
    match socket.recv_from(&mut buf) {
        Ok((n, _src)) => parse_pmap_getport_udp_reply(&buf[..n]),
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

    // Build the PMAP_DUMP UDP request.
    let mut xdr = Xdr::new();
    xdr.u32(0); // XID placeholder
    xdr.u32(RPC_CALL);
    xdr.u32(RPC_VERSION);
    xdr.u32(PROG_PORTMAP);
    xdr.u32(VERS_PORTMAP);
    xdr.u32(PROC_PMAP_DUMP);
    xdr.u32(AUTH_NULL);
    xdr.u32(0);
    xdr.u32(AUTH_NULL);
    xdr.u32(0);
    xdr.into_bytes();

    // Send broadcast to port 111.
    let broadcast_addr: SocketAddr = "255.255.255.255:111".parse().unwrap();
    socket.send_to(&pmap_dump_udp_payload(), broadcast_addr)?;

    // Collect responses.
    let mut mappings = Vec::new();
    let mut buf = [0u8; MAX_RECORD];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, _src)) => {
                if n >= 4 {
                    if let Ok(reply) = parse_pmap_dump_udp_reply(&buf[..n]) {
                        mappings.extend(reply);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
    Ok(mappings)
}

/// Build a PMAP_DUMP UDP request payload.
fn pmap_dump_udp_payload() -> Vec<u8> {
    let mut xdr = Xdr::new();
    xdr.u32(0); // XID placeholder
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

/// Parse a PMAP_DUMP UDP reply payload (including the 4-byte XID prefix).
fn parse_pmap_dump_udp_reply(buf: &[u8]) -> Result<Vec<PortMapping>> {
    if buf.len() < 4 {
        return Err(Error::Malformed(
            "PMAP_DUMP UDP reply too short".to_string(),
        ));
    }
    let mut reader = XdrReader::new(&buf[4..]); // skip XID
    let direction = reader.u32()?;
    if direction != RPC_REPLY {
        return Err(Error::Rpc(
            "expected REPLY in UDP pmap dump response".to_string(),
        ));
    }
    let reply_state = reader.u32()?;
    if reply_state != MSG_ACCEPTED {
        return Err(Error::Rpc("PMAP_DUMP UDP reply denied".to_string()));
    }
    let accept_state = reader.u32()?;
    if accept_state != SUCCESS {
        return Err(Error::Rpc("PMAP_DUMP UDP call failed".to_string()));
    }

    let mut mappings = Vec::new();
    loop {
        let remaining = reader.remaining();
        if remaining == 0 {
            break;
        }
        // Each entry is followed by a 4-byte "follows" flag.
        let prog = reader.u32()?;
        let vers = reader.u32()?;
        let prot = reader.u32()?;
        let port = reader.u32()?;
        let follows = reader.u32()?;
        mappings.push(PortMapping {
            prog,
            vers,
            prot,
            port: port as u16,
        });
        if follows == 0 {
            break;
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
    fn test_pmap_getport_unregistered_returns_none() {
        // Craft a PMAP_GETPORT reply with port=0 (not registered).
        // XID=0, REPLY, MSG_ACCEPTED, SUCCESS, port=0
        let reply: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, // XID
            0x00, 0x00, 0x00, 0x01, // REPLY
            0x00, 0x00, 0x00, 0x00, // MSG_ACCEPTED
            0x00, 0x00, 0x00, 0x00, // SUCCESS
            0x00, 0x00, 0x00, 0x00, // port = 0
        ];
        let result = parse_pmap_getport_udp_reply(reply).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_pmap_dump_decodes_reply() {
        // Craft a PMAP_DUMP reply with one entry.
        // XID=0, REPLY, MSG_ACCEPTED, SUCCESS
        // prog=100000, vers=2, prot=6(TCP), port=111, follows=0
        let reply: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, // XID
            0x00, 0x00, 0x00, 0x01, // REPLY
            0x00, 0x00, 0x00, 0x00, // MSG_ACCEPTED
            0x00, 0x00, 0x00, 0x00, // SUCCESS
            0x00, 0x01, 0x86, 0xA0, // prog = 100000
            0x00, 0x00, 0x00, 0x02, // vers = 2
            0x00, 0x00, 0x00, 0x06, // prot = TCP
            0x00, 0x00, 0x00, 0x6F, // port = 111
            0x00, 0x00, 0x00, 0x00, // follows = 0 (last entry)
        ];
        let result = parse_pmap_dump_udp_reply(reply).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].prog, 100_000);
        assert_eq!(result[0].vers, 2);
        assert_eq!(result[0].prot, 6);
        assert_eq!(result[0].port, 111);
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
