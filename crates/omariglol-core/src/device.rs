//! The instrument model shared by the CLI, the GUI and the shell plugin.

use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How an instrument was located.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryMethod {
    /// VXI-11 broadcast against the portmapper (what `lxi discover` does).
    Vxi11Broadcast,
    /// mDNS / DNS-SD service browsing (`lxi discover --mdns`).
    Mdns,
    /// Typed in by hand because the instrument is on another subnet.
    Manual,
}

impl fmt::Display for DiscoveryMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!("human-readable discovery method")
    }
}

/// A discovered or manually configured LXI instrument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Primary IPv4 address of the instrument.
    pub address: IpAddr,
    /// Address as the user typed it, when the entry is manual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// VXI-11 core channel port from the portmapper (usually 1024+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vxi11_port: Option<u16>,
    /// SCPI raw socket port (5025 by convention).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_port: Option<u16>,
    /// mDNS instance name, when discovered that way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mdns_name: Option<String>,
    /// The VXI-11 device name to open, normally `inst0`.
    #[serde(default = "default_device_name")]
    pub device_name: String,
    pub method: DiscoveryMethod,
}

/// The VXI-11 device name convention: `inst0` is the instrument itself.
pub(crate) fn default_device_name() -> String {
    "inst0".to_string()
}

impl Device {
    /// Build a manual device entry for a host the user typed in.
    pub fn manual(address: IpAddr) -> Self {
        let _ = address;
        todo!("construct a manual Device entry")
    }

    /// The address in `lxi`'s display form: `IP (hostname)` when known.
    pub fn display_address(&self) -> String {
        let _ = self;
        todo!("format the address for display")
    }

    /// Best-effort label for lists and bar widgets.
    pub fn label(&self) -> String {
        let _ = self;
        todo!("pick the friendliest available name")
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!("render a device for the CLI")
    }
}

/// Which transport to use when talking to an instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    /// VXI-11 over ONC RPC, via the portmapper on port 111.
    Vxi11,
    /// SCPI on a raw TCP socket, port 5025 by convention.
    Raw,
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!("human-readable transport name")
    }
}

/// Where to connect, and how.
#[derive(Debug, Clone)]
pub struct Target {
    pub address: IpAddr,
    pub transport: Transport,
    /// Overrides the transport's default port when set.
    pub port: Option<u16>,
    pub device_name: String,
    /// Socket timeout applied to connect/read/write.
    pub timeout: Duration,
}

impl Target {
    pub fn new(address: IpAddr, transport: Transport) -> Self {
        let _ = (address, transport);
        todo!("build a target with default port and timeout")
    }

    // The stub does not mutate yet; the builder contract still takes `self`.
    #[allow(unused_mut)]
    pub fn with_port(mut self, port: u16) -> Self {
        let _ = port;
        todo!("override the port")
    }

    #[allow(unused_mut)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        let _ = timeout;
        todo!("override the timeout")
    }

    /// The port this target will actually dial.
    pub fn effective_port(&self) -> u16 {
        let _ = self;
        todo!("resolve the transport's default port")
    }

    /// Derive a target from a discovered device.
    pub fn from_device(device: &Device, transport: Transport) -> Self {
        let _ = (device, transport);
        todo!("carry the discovered ports across to the target")
    }
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn test_discovery_method_display() {
        todo!("human-readable discovery method")
    }

    #[test]
    #[ignore]
    fn test_device_manual() {
        todo!("construct a manual Device entry")
    }

    #[test]
    #[ignore]
    fn test_device_display_address() {
        todo!("format the address for display")
    }

    #[test]
    #[ignore]
    fn test_device_label() {
        todo!("pick the friendliest available name")
    }

    #[test]
    #[ignore]
    fn test_device_display() {
        todo!("render a device for the CLI")
    }

    #[test]
    #[ignore]
    fn test_transport_display() {
        todo!("human-readable transport name")
    }

    #[test]
    #[ignore]
    fn test_target_new() {
        todo!("build a target with default port and timeout")
    }

    #[test]
    #[ignore]
    fn test_target_effective_port() {
        todo!("resolve the transport's default port")
    }

    #[test]
    #[ignore]
    fn test_target_from_device() {
        todo!("carry the discovered ports across to the target")
    }
}
