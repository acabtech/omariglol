//! VXI-11 client: the ONC RPC `device_core` channel that `lxi scpi` uses by
//! default.
//!
//! Flow: resolve the `DEVICE_CORE` program through the portmapper on port 111,
//! `create_link` to obtain a link id, then `device_write` / `device_read` pairs.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use super::ScpiTransport;
use super::rpc::{self, IPPROTO_TCP, RpcClient, Xdr, XdrReader};
use crate::device::Target;
use crate::error::{Error, Result, vxi11_error_name};

/// VXI-11 `DEVICE_CORE` program number.
pub const DEVICE_CORE: u32 = 0x0006_07AF;
/// VXI-11 `DEVICE_CORE` version.
pub const DEVICE_CORE_VERSION: u32 = 1;

// Procedure numbers from the VXI-11 specification.
pub const PROC_CREATE_LINK: u32 = 10;
pub const PROC_DEVICE_WRITE: u32 = 11;
pub const PROC_DEVICE_READ: u32 = 12;
pub const PROC_DEVICE_READSTB: u32 = 13;
pub const PROC_DEVICE_TRIGGER: u32 = 14;
pub const PROC_DEVICE_CLEAR: u32 = 15;
pub const PROC_DEVICE_REMOTE: u32 = 16;
pub const PROC_DEVICE_LOCAL: u32 = 17;
pub const PROC_DEVICE_LOCK: u32 = 18;
pub const PROC_DEVICE_UNLOCK: u32 = 19;
pub const PROC_DEVICE_ENABLE_SRQ: u32 = 20;
pub const PROC_DEVICE_DOCMD: u32 = 22;
pub const PROC_DESTROY_LINK: u32 = 23;
pub const PROC_CREATE_INTR_CHAN: u32 = 25;
pub const PROC_DESTROY_INTR_CHAN: u32 = 26;

// `Device_Flags` bits.
pub const FLAG_WAIT_LOCK: i32 = 1;
pub const FLAG_END: i32 = 8;
pub const FLAG_TERMCHRSET: i32 = 128;

// `Device_ReadResp.reason` values.
pub const REASON_REQCNT: i32 = 1;
pub const REASON_CHR: i32 = 2;
pub const REASON_END: i32 = 4;

/// The default per-request read size, matching liblxi.
pub const DEFAULT_READ_SIZE: u32 = 1024 * 128;

/// The client id presented in `create_link`; liblxi uses 0.
const CLIENT_ID: i32 = 0;

/// The SCPI terminator: commands are newline-terminated and responses are read
/// up to a newline.
const TERMINATOR: u8 = b'\n';

/// Convert a [`Duration`] to the millisecond count VXI-11 timeouts use,
/// saturating rather than wrapping.
fn timeout_ms(timeout: Duration) -> u32 {
    let ms = timeout.as_millis();
    if ms > u32::MAX as u128 {
        u32::MAX
    } else {
        ms as u32
    }
}

// ---------------------------------------------------------------------------
// Request encoders
//
// Each procedure's arguments are encoded by a small pure function so the wire
// format can be unit-tested without a live instrument.
// ---------------------------------------------------------------------------

/// `Create_LinkParms { clientId, lockDevice, lock_timeout, device }`.
fn encode_create_link(device: &str) -> Vec<u8> {
    let mut x = Xdr::new();
    x.i32(CLIENT_ID);
    x.bool(false);
    x.u32(0); // lock_timeout
    x.string(device);
    x.into_bytes()
}

/// `Device_WriteParms { lid, io_timeout, lock_timeout, flags, data }`.
fn encode_device_write(lid: i32, io_timeout: u32, flags: i32, data: &[u8]) -> Vec<u8> {
    let mut x = Xdr::new();
    x.i32(lid);
    x.u32(io_timeout);
    x.u32(0); // lock_timeout
    x.i32(flags);
    x.opaque(data);
    x.into_bytes()
}

/// `Device_ReadParms { lid, requestSize, io_timeout, lock_timeout, flags, termChar }`.
fn encode_device_read(
    lid: i32,
    request_size: u32,
    io_timeout: u32,
    flags: i32,
    term_char: i32,
) -> Vec<u8> {
    let mut x = Xdr::new();
    x.i32(lid);
    x.u32(request_size);
    x.u32(io_timeout);
    x.u32(0); // lock_timeout
    x.i32(flags);
    x.i32(term_char);
    x.into_bytes()
}

/// `Device_GenericParms { lid, flags, lock_timeout, io_timeout }`.
///
/// Shared by `device_readstb`, `device_trigger`, `device_clear` and
/// `device_local`.
fn encode_generic(lid: i32, flags: i32, lock_timeout: u32, io_timeout: u32) -> Vec<u8> {
    let mut x = Xdr::new();
    x.i32(lid);
    x.i32(flags);
    x.u32(lock_timeout);
    x.u32(io_timeout);
    x.into_bytes()
}

/// `Device_LockParms { lid, flags, lock_timeout }`.
fn encode_lock(lid: i32, flags: i32, lock_timeout: u32) -> Vec<u8> {
    let mut x = Xdr::new();
    x.i32(lid);
    x.i32(flags);
    x.u32(lock_timeout);
    x.into_bytes()
}

/// `Device_Link { lid }`, used by `device_unlock` and `destroy_link`.
fn encode_link(lid: i32) -> Vec<u8> {
    let mut x = Xdr::new();
    x.i32(lid);
    x.into_bytes()
}

/// `Device_RemoteParms { lid, func }` where `func` is a `Device_RemoteFunc`.
///
/// We do not run an interrupt channel, so the remote function is all zeroes
/// with the TCP address family, matching liblxi.
fn encode_remote(lid: i32) -> Vec<u8> {
    let mut x = Xdr::new();
    x.i32(lid);
    x.u32(0); // hostAddr
    x.u32(0); // hostPort
    x.u32(0); // progNum
    x.u32(0); // progVers
    x.i32(0); // progFamily = DEVICE_TCP
    x.into_bytes()
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// A live VXI-11 connection to an instrument.
pub struct Vxi11Transport {
    rpc: RpcClient,
    lid: i32,
    read_size: u32,
    timeout: Duration,
    last_reason: i32,
    addr: SocketAddr,
    closed: bool,
}

impl Vxi11Transport {
    /// Open a VXI-11 link to `target`.
    pub fn connect(target: &Target) -> Result<Self> {
        // Resolve the core channel port: an explicit override wins, otherwise
        // ask the portmapper on port 111.
        let port = match target.port {
            Some(port) => port,
            None => rpc::pmap_getport(
                target.address,
                DEVICE_CORE,
                DEVICE_CORE_VERSION,
                IPPROTO_TCP,
                target.timeout,
            )?
            .ok_or_else(|| Error::Portmap {
                program: DEVICE_CORE,
                message: "DEVICE_CORE is not registered".to_string(),
            })?,
        };

        let addr = SocketAddr::new(target.address, port);
        let mut rpc = RpcClient::connect(addr, target.timeout)?;

        let params = encode_create_link(&target.device_name);
        let reply = rpc.call(DEVICE_CORE, DEVICE_CORE_VERSION, PROC_CREATE_LINK, &params)?;
        let mut reader = XdrReader::new(&reply);
        let error = reader.i32()?;
        if error != 0 {
            return Err(Error::Vxi11 {
                code: error,
                message: vxi11_error_name(error).to_string(),
            });
        }
        let lid = reader.i32()?;
        let _abort_port = reader.u32()?;
        let max_recv_size = reader.u32()?;
        // Prefer the instrument's advertised maximum, falling back to the
        // liblxi-compatible default when it reports none.
        let read_size = if max_recv_size > 0 {
            max_recv_size
        } else {
            DEFAULT_READ_SIZE
        };

        Ok(Self {
            rpc,
            lid,
            read_size,
            timeout: target.timeout,
            last_reason: 0,
            addr,
            closed: false,
        })
    }

    /// Issue a procedure whose only result is a `Device_ErrorCode`.
    fn call_error(&mut self, proc: u32, params: &[u8]) -> Result<()> {
        let reply = self
            .rpc
            .call(DEVICE_CORE, DEVICE_CORE_VERSION, proc, params)?;
        let mut reader = XdrReader::new(&reply);
        let error = reader.i32()?;
        if error != 0 {
            return Err(Error::Vxi11 {
                code: error,
                message: vxi11_error_name(error).to_string(),
            });
        }
        Ok(())
    }

    /// `device_trigger` — the hardware trigger.
    pub fn trigger(&mut self) -> Result<()> {
        let params = encode_generic(self.lid, 0, 0, timeout_ms(self.timeout));
        self.call_error(PROC_DEVICE_TRIGGER, &params)
    }

    /// `device_clear` — clears the device's input/output buffers.
    pub fn clear(&mut self) -> Result<()> {
        let params = encode_generic(self.lid, 0, 0, timeout_ms(self.timeout));
        self.call_error(PROC_DEVICE_CLEAR, &params)
    }

    /// `device_remote` / `device_local` — front-panel lockout toggles.
    pub fn set_remote(&mut self, remote: bool) -> Result<()> {
        if remote {
            let params = encode_remote(self.lid);
            self.call_error(PROC_DEVICE_REMOTE, &params)
        } else {
            let params = encode_generic(self.lid, 0, 0, timeout_ms(self.timeout));
            self.call_error(PROC_DEVICE_LOCAL, &params)
        }
    }

    /// `device_lock` / `device_unlock`.
    pub fn set_lock(&mut self, lock: bool) -> Result<()> {
        if lock {
            let params = encode_lock(self.lid, 0, 0);
            self.call_error(PROC_DEVICE_LOCK, &params)
        } else {
            let params = encode_link(self.lid);
            self.call_error(PROC_DEVICE_UNLOCK, &params)
        }
    }

    /// Read the status byte — the VXI-11 equivalent of `*STB?`.
    pub fn read_status_byte(&mut self) -> Result<u8> {
        let params = encode_generic(self.lid, 0, 0, timeout_ms(self.timeout));
        let reply = self.rpc.call(
            DEVICE_CORE,
            DEVICE_CORE_VERSION,
            PROC_DEVICE_READSTB,
            &params,
        )?;
        let mut reader = XdrReader::new(&reply);
        let error = reader.i32()?;
        if error != 0 {
            return Err(Error::Vxi11 {
                code: error,
                message: vxi11_error_name(error).to_string(),
            });
        }
        Ok(reader.u32()? as u8)
    }

    /// Reason code from the most recent read, so callers can tell whether the
    /// instrument ended on a termchar or on the END indicator.
    pub fn last_reason(&self) -> i32 {
        self.last_reason
    }
}

impl ScpiTransport for Vxi11Transport {
    fn send(&mut self, command: &str) -> Result<()> {
        let mut data = command.as_bytes().to_vec();
        if data.last() != Some(&TERMINATOR) {
            data.push(TERMINATOR);
        }
        let params = encode_device_write(self.lid, timeout_ms(self.timeout), FLAG_END, &data);
        let reply = self
            .rpc
            .call(DEVICE_CORE, DEVICE_CORE_VERSION, PROC_DEVICE_WRITE, &params)?;
        let mut reader = XdrReader::new(&reply);
        let error = reader.i32()?;
        if error != 0 {
            return Err(Error::Vxi11 {
                code: error,
                message: vxi11_error_name(error).to_string(),
            });
        }
        let _written = reader.u32()?;
        Ok(())
    }

    fn receive(&mut self) -> Result<Vec<u8>> {
        let params = encode_device_read(
            self.lid,
            self.read_size,
            timeout_ms(self.timeout),
            FLAG_TERMCHRSET,
            TERMINATOR as i32,
        );
        let reply = self
            .rpc
            .call(DEVICE_CORE, DEVICE_CORE_VERSION, PROC_DEVICE_READ, &params)?;
        let mut reader = XdrReader::new(&reply);
        let error = reader.i32()?;
        if error != 0 {
            return Err(Error::Vxi11 {
                code: error,
                message: vxi11_error_name(error).to_string(),
            });
        }
        let reason = reader.i32()?;
        self.last_reason = reason;
        let mut data = reader.opaque()?.to_vec();
        // Strip the transport terminator, as the trait contract requires.
        if data.last() == Some(&TERMINATOR) {
            data.pop();
        }
        if data.last() == Some(&b'\r') {
            data.pop();
        }
        Ok(data)
    }

    fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let params = encode_link(self.lid);
        self.call_error(PROC_DESTROY_LINK, &params)
    }

    fn description(&self) -> String {
        format!("VXI-11 {} (lid {})", self.addr, self.lid)
    }
}

impl Drop for Vxi11Transport {
    fn drop(&mut self) {
        // Best effort: a failure to destroy the link is not actionable here.
        let _ = self.close();
    }
}

/// Convenience helper used by discovery: does this host expose VXI-11?
pub fn has_vxi11(addr: IpAddr, timeout: Duration) -> bool {
    matches!(
        rpc::pmap_getport(addr, DEVICE_CORE, DEVICE_CORE_VERSION, IPPROTO_TCP, timeout),
        Ok(Some(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_link_encoding() {
        let bytes = encode_create_link("inst0");
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), CLIENT_ID);
        assert!(!reader.bool().unwrap());
        assert_eq!(reader.u32().unwrap(), 0);
        assert_eq!(reader.string().unwrap(), "inst0");
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_device_write_encoding() {
        let bytes = encode_device_write(7, 1000, FLAG_END, b"*IDN?\n");
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), 7);
        assert_eq!(reader.u32().unwrap(), 1000);
        assert_eq!(reader.u32().unwrap(), 0);
        assert_eq!(reader.i32().unwrap(), FLAG_END);
        assert_eq!(reader.opaque().unwrap(), b"*IDN?\n");
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_device_read_encoding() {
        let bytes = encode_device_read(
            7,
            DEFAULT_READ_SIZE,
            1000,
            FLAG_TERMCHRSET,
            TERMINATOR as i32,
        );
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), 7);
        assert_eq!(reader.u32().unwrap(), DEFAULT_READ_SIZE);
        assert_eq!(reader.u32().unwrap(), 1000);
        assert_eq!(reader.u32().unwrap(), 0);
        assert_eq!(reader.i32().unwrap(), FLAG_TERMCHRSET);
        assert_eq!(reader.i32().unwrap(), TERMINATOR as i32);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_device_readstb_encoding() {
        let bytes = encode_generic(3, 0, 0, 500);
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), 3);
        assert_eq!(reader.i32().unwrap(), 0);
        assert_eq!(reader.u32().unwrap(), 0);
        assert_eq!(reader.u32().unwrap(), 500);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_device_trigger_encoding() {
        let bytes = encode_generic(3, 0, 0, 500);
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), 3);
        assert_eq!(reader.i32().unwrap(), 0);
        assert_eq!(reader.u32().unwrap(), 0);
        assert_eq!(reader.u32().unwrap(), 500);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_device_clear_encoding() {
        let bytes = encode_generic(9, 0, 0, 250);
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), 9);
        assert_eq!(reader.i32().unwrap(), 0);
        assert_eq!(reader.u32().unwrap(), 0);
        assert_eq!(reader.u32().unwrap(), 250);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_device_lock_unlock_encoding() {
        let lock = encode_lock(5, 0, 0);
        let mut reader = XdrReader::new(&lock);
        assert_eq!(reader.i32().unwrap(), 5);
        assert_eq!(reader.i32().unwrap(), 0);
        assert_eq!(reader.u32().unwrap(), 0);
        assert_eq!(reader.remaining(), 0);

        let unlock = encode_link(5);
        let mut reader = XdrReader::new(&unlock);
        assert_eq!(reader.i32().unwrap(), 5);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_device_remote_local_encoding() {
        let remote = encode_remote(5);
        let mut reader = XdrReader::new(&remote);
        assert_eq!(reader.i32().unwrap(), 5);
        assert_eq!(reader.u32().unwrap(), 0); // hostAddr
        assert_eq!(reader.u32().unwrap(), 0); // hostPort
        assert_eq!(reader.u32().unwrap(), 0); // progNum
        assert_eq!(reader.u32().unwrap(), 0); // progVers
        assert_eq!(reader.i32().unwrap(), 0); // progFamily = DEVICE_TCP
        assert_eq!(reader.remaining(), 0);

        let local = encode_generic(5, 0, 0, 100);
        let mut reader = XdrReader::new(&local);
        assert_eq!(reader.i32().unwrap(), 5);
        assert_eq!(reader.remaining(), 12);
    }

    #[test]
    fn test_destroy_link_encoding() {
        let bytes = encode_link(42);
        let mut reader = XdrReader::new(&bytes);
        assert_eq!(reader.i32().unwrap(), 42);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_has_vxi11_unreachable_is_false() {
        // 192.0.2.0/24 is TEST-NET-1 and is never routable, so the helper must
        // report false rather than panic.
        let result = has_vxi11("192.0.2.1".parse().unwrap(), Duration::from_millis(50));
        assert!(!result);
    }
}
