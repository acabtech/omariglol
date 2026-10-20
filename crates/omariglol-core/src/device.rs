//! The instrument model shared by the CLI, the GUI and the shell plugin.

use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::transport::raw::DEFAULT_RAW_PORT;

/// The portmapper port, dialed to resolve the VXI-11 core channel.
///
/// VXI-11 has no fixed device port: the core channel is assigned dynamically
/// and looked up through the portmapper, which always lives on port 111.
pub const DEFAULT_VXI11_PORT: u16 = 111;

/// Default socket timeout applied to a freshly built [`Target`].
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

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
        let name = match self {
            DiscoveryMethod::Vxi11Broadcast => "vxi11-broadcast",
            DiscoveryMethod::Mdns => "mdns",
            DiscoveryMethod::Manual => "manual",
        };
        f.write_str(name)
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
        Self {
            address,
            hostname: None,
            vxi11_port: None,
            raw_port: None,
            mdns_name: None,
            device_name: default_device_name(),
            method: DiscoveryMethod::Manual,
        }
    }

    /// The address in `lxi`'s display form: `IP (hostname)` when known.
    pub fn display_address(&self) -> String {
        match &self.hostname {
            Some(hostname) => format!("{} ({})", self.address, hostname),
            None => self.address.to_string(),
        }
    }

    /// Best-effort label for lists and bar widgets.
    ///
    /// Prefers the mDNS instance name, then a resolved hostname, and finally
    /// falls back to the bare IP address.
    pub fn label(&self) -> String {
        if let Some(name) = self.mdns_name.as_deref() {
            return name.to_string();
        }
        if let Some(hostname) = self.hostname.as_deref() {
            return hostname.to_string();
        }
        self.address.to_string()
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}]", self.label(), self.method)
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
        let name = match self {
            Transport::Vxi11 => "vxi11",
            Transport::Raw => "raw",
        };
        f.write_str(name)
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
        Self {
            address,
            transport,
            port: None,
            device_name: default_device_name(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The port this target will actually dial.
    ///
    /// An explicit override always wins. Otherwise the transport default is
    /// used: the portmapper port for VXI-11 (the core channel is resolved from
    /// there) and 5025 for the raw socket.
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(match self.transport {
            Transport::Vxi11 => DEFAULT_VXI11_PORT,
            Transport::Raw => DEFAULT_RAW_PORT,
        })
    }

    /// Derive a target from a discovered device.
    ///
    /// The port discovered for the requested transport is carried across; the
    /// device name and address come from the device, and the timeout is the
    /// default.
    pub fn from_device(device: &Device, transport: Transport) -> Self {
        let port = match transport {
            Transport::Vxi11 => device.vxi11_port,
            Transport::Raw => device.raw_port,
        };
        Self {
            address: device.address,
            transport,
            port,
            device_name: device.device_name.clone(),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn test_discovery_method_display() {
        assert_eq!(
            DiscoveryMethod::Vxi11Broadcast.to_string(),
            "vxi11-broadcast"
        );
        assert_eq!(DiscoveryMethod::Mdns.to_string(), "mdns");
        assert_eq!(DiscoveryMethod::Manual.to_string(), "manual");
    }

    #[test]
    fn test_device_manual() {
        let device = Device::manual(ip("192.168.1.10"));
        assert_eq!(device.address, ip("192.168.1.10"));
        assert_eq!(device.method, DiscoveryMethod::Manual);
        assert_eq!(device.device_name, "inst0");
        assert!(device.hostname.is_none());
        assert!(device.vxi11_port.is_none());
        assert!(device.raw_port.is_none());
        assert!(device.mdns_name.is_none());
    }

    #[test]
    fn test_device_display_address() {
        let mut device = Device::manual(ip("192.168.1.10"));
        assert_eq!(device.display_address(), "192.168.1.10");
        device.hostname = Some("scope.local".to_string());
        assert_eq!(device.display_address(), "192.168.1.10 (scope.local)");
    }

    #[test]
    fn test_device_label() {
        let mut device = Device::manual(ip("192.168.1.10"));
        // Falls back to the bare IP.
        assert_eq!(device.label(), "192.168.1.10");
        // A hostname beats the IP.
        device.hostname = Some("scope.local".to_string());
        assert_eq!(device.label(), "scope.local");
        // The mDNS name beats the hostname.
        device.mdns_name = Some("Rigol DS1054Z".to_string());
        assert_eq!(device.label(), "Rigol DS1054Z");
    }

    #[test]
    fn test_device_display() {
        let mut device = Device::manual(ip("192.168.1.10"));
        device.mdns_name = Some("scope".to_string());
        assert_eq!(device.to_string(), "scope [manual]");
    }

    #[test]
    fn test_transport_display() {
        assert_eq!(Transport::Vxi11.to_string(), "vxi11");
        assert_eq!(Transport::Raw.to_string(), "raw");
    }

    #[test]
    fn test_target_new() {
        let target = Target::new(ip("192.168.1.10"), Transport::Raw);
        assert_eq!(target.address, ip("192.168.1.10"));
        assert_eq!(target.transport, Transport::Raw);
        assert_eq!(target.port, None);
        assert_eq!(target.device_name, "inst0");
        assert_eq!(target.timeout, DEFAULT_TIMEOUT);
    }

    #[test]
    fn test_target_builders() {
        let target = Target::new(ip("192.168.1.10"), Transport::Raw)
            .with_port(1234)
            .with_timeout(Duration::from_millis(250));
        assert_eq!(target.port, Some(1234));
        assert_eq!(target.timeout, Duration::from_millis(250));
    }

    #[test]
    fn test_target_effective_port() {
        let raw = Target::new(ip("192.168.1.10"), Transport::Raw);
        assert_eq!(raw.effective_port(), DEFAULT_RAW_PORT);

        let vxi11 = Target::new(ip("192.168.1.10"), Transport::Vxi11);
        assert_eq!(vxi11.effective_port(), DEFAULT_VXI11_PORT);

        let overridden = raw.with_port(9999);
        assert_eq!(overridden.effective_port(), 9999);
    }

    #[test]
    fn test_target_from_device() {
        let mut device = Device::manual(ip("192.168.1.10"));
        device.vxi11_port = Some(1024);
        device.raw_port = Some(5025);

        let vxi11 = Target::from_device(&device, Transport::Vxi11);
        assert_eq!(vxi11.address, ip("192.168.1.10"));
        assert_eq!(vxi11.port, Some(1024));
        assert_eq!(vxi11.device_name, "inst0");

        let raw = Target::from_device(&device, Transport::Raw);
        assert_eq!(raw.port, Some(5025));
    }
}
