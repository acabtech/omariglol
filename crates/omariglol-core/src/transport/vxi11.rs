//! VXI-11 client: the ONC RPC `device_core` channel that `lxi scpi` uses by
//! default.
//!
//! Flow: resolve the `DEVICE_CORE` program through the portmapper on port 111,
//! `create_link` to obtain a link id, then `device_write` / `device_read` pairs.

use std::net::IpAddr;
use std::time::Duration;

use super::ScpiTransport;
use crate::device::Target;
use crate::error::Result;

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

/// A live VXI-11 connection to an instrument.
pub struct Vxi11Transport {
    rpc: super::rpc::RpcClient,
    lid: i32,
    read_size: u32,
    timeout: Duration,
    last_reason: i32,
}

impl Vxi11Transport {
    /// Open a VXI-11 link to `target`.
    pub fn connect(target: &Target) -> Result<Self> {
        let _ = target;
        todo!("resolve the core channel port, then create_link")
    }

    /// `device_trigger` — the hardware trigger.
    pub fn trigger(&mut self) -> Result<()> {
        todo!("call device_trigger")
    }

    /// `device_clear` — clears the device's input/output buffers.
    pub fn clear(&mut self) -> Result<()> {
        todo!("call device_clear")
    }

    /// `device_remote` / `device_local` — front-panel lockout toggles.
    pub fn set_remote(&mut self, remote: bool) -> Result<()> {
        let _ = remote;
        todo!("call device_remote or device_local")
    }

    /// `device_lock` / `device_unlock`.
    pub fn set_lock(&mut self, lock: bool) -> Result<()> {
        let _ = lock;
        todo!("call device_lock or device_unlock")
    }

    /// Read the status byte — the VXI-11 equivalent of `*STB?`.
    pub fn read_status_byte(&mut self) -> Result<u8> {
        todo!("call device_readstb")
    }

    /// Reason code from the most recent read, so callers can tell whether the
    /// instrument ended on a termchar or on the END indicator.
    pub fn last_reason(&self) -> i32 {
        let _ = self;
        todo!("return the cached read reason")
    }
}

impl ScpiTransport for Vxi11Transport {
    fn send(&mut self, command: &str) -> Result<()> {
        let _ = command;
        todo!("device_write with the END flag")
    }

    fn receive(&mut self) -> Result<Vec<u8>> {
        todo!("device_read with a termchar and the configured read size")
    }

    fn close(&mut self) -> Result<()> {
        todo!("destroy_link")
    }

    fn description(&self) -> String {
        let _ = self;
        todo!("describe the link")
    }
}

/// Convenience helper used by discovery: does this host expose VXI-11?
pub fn has_vxi11(addr: IpAddr, timeout: Duration) -> bool {
    let _ = (addr, timeout);
    todo!("ask the portmapper whether DEVICE_CORE is registered")
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn test_create_link_encoding() {
        todo!("request encoding for create_link")
    }

    #[test]
    #[ignore]
    fn test_device_write_encoding() {
        todo!("request encoding for device_write")
    }

    #[test]
    #[ignore]
    fn test_device_read_encoding() {
        todo!("request encoding for device_read")
    }

    #[test]
    #[ignore]
    fn test_device_readstb_encoding() {
        todo!("request encoding for device_readstb")
    }

    #[test]
    #[ignore]
    fn test_device_trigger_encoding() {
        todo!("request encoding for device_trigger")
    }

    #[test]
    #[ignore]
    fn test_device_clear_encoding() {
        todo!("request encoding for device_clear")
    }

    #[test]
    #[ignore]
    fn test_device_lock_unlock_encoding() {
        todo!("request encoding for device_lock/device_unlock")
    }

    #[test]
    #[ignore]
    fn test_device_remote_local_encoding() {
        todo!("request encoding for device_remote/device_local")
    }

    #[test]
    #[ignore]
    fn test_destroy_link_encoding() {
        todo!("request encoding for destroy_link")
    }

    #[test]
    #[ignore]
    fn test_has_vxi11() {
        todo!("has_vxi11 helper")
    }
}
