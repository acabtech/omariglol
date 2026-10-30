//! The instrument model shared by the CLI, the GUI and the shell plugin.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
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
    /// A direct portmapper query, used when broadcast and raw SCPI are
    /// unavailable but the instrument still answers `DEVICE_CORE`.
    Vxi11Portmap,
    /// mDNS / DNS-SD service browsing (`lxi discover --mdns`).
    Mdns,
    /// A direct SCPI `*IDN?` probe over TCP on a known or scanned address.
    ScpiTcp,
    /// Enumeration of the local USB bus.
    Usb,
    /// Typed in by hand because the instrument is on another subnet.
    Manual,
}

impl fmt::Display for DiscoveryMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            DiscoveryMethod::Vxi11Broadcast => "vxi11-broadcast",
            DiscoveryMethod::Vxi11Portmap => "vxi11-portmap",
            DiscoveryMethod::Mdns => "mdns",
            DiscoveryMethod::ScpiTcp => "scpi-tcp",
            DiscoveryMethod::Manual => "manual",
            DiscoveryMethod::Usb => "usb",
        };
        f.write_str(name)
    }
}

/// How to find a USB instrument: by vendor/product id, optionally pinned to a
/// serial number or a bus/device pair when several identical units are attached.
///
/// The `usb:0xVVVV:0xPPPP[:SERIAL|BUS.ADDR]` display form round-trips through
/// [`UsbSelector::to_string`] and [`UsbSelector::parse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbSelector {
    /// USB vendor id.
    pub vendor: u16,
    /// USB product id.
    pub product: u16,
    /// Serial number, to disambiguate identical models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Explicit bus number, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<u8>,
    /// Explicit device address on `bus`, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<u8>,
}

impl UsbSelector {
    /// A selector keyed only on the vendor/product pair.
    pub fn new(vendor: u16, product: u16) -> Self {
        Self {
            vendor,
            product,
            serial: None,
            bus: None,
            address: None,
        }
    }

    /// Pin the selector to a serial number.
    pub fn with_serial(mut self, serial: impl Into<String>) -> Self {
        self.serial = Some(serial.into());
        self
    }

    /// Parse the `usb:0xVVVV:0xPPPP[:SERIAL|BUS.ADDR]` address form.
    ///
    /// The ids accept both `0x`-prefixed hexadecimal and plain decimal.
    pub fn parse(input: &str) -> Option<Self> {
        let rest = input.strip_prefix("usb:")?;
        let mut parts = rest.split(':');
        let vendor = parse_u16(parts.next()?)?;
        let product = parse_u16(parts.next()?)?;
        let mut selector = Self::new(vendor, product);
        if let Some(tail) = parts.next() {
            if let Some((bus, addr)) = tail.split_once('.') {
                selector.bus = bus.parse().ok();
                selector.address = addr.parse().ok();
            } else if !tail.is_empty() {
                selector.serial = Some(tail.to_string());
            }
        }
        Some(selector)
    }
}

fn parse_u16(token: &str) -> Option<u16> {
    let token = token.trim();
    match token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => token.parse().ok(),
    }
}

impl fmt::Display for UsbSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "usb:0x{:04X}:0x{:04X}", self.vendor, self.product)?;
        if let Some(serial) = &self.serial {
            write!(f, ":{serial}")?;
        } else if let (Some(bus), Some(addr)) = (self.bus, self.address) {
            write!(f, ":{bus}.{addr}")?;
        }
        Ok(())
    }
}

/// A discovered or manually configured LXI instrument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Primary IPv4 address of the instrument.
    ///
    /// For a USB instrument this is the unspecified address `0.0.0.0`; use
    /// [`Device::usb`] instead of poking at it.
    pub address: IpAddr,
    /// USB selector, when the instrument is reached over USBTMC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb: Option<UsbSelector>,
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
            usb: None,
            hostname: None,
            vxi11_port: None,
            raw_port: None,
            mdns_name: None,
            device_name: default_device_name(),
            method: DiscoveryMethod::Manual,
        }
    }

    /// Build a manual device entry for a USB instrument.
    pub fn usb(selector: UsbSelector) -> Self {
        Self {
            address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            usb: Some(selector),
            hostname: None,
            vxi11_port: None,
            raw_port: None,
            mdns_name: None,
            device_name: default_device_name(),
            method: DiscoveryMethod::Manual,
        }
    }

    /// Build a device from a user-supplied address and transport.
    ///
    /// A USB address uses the `usb:0xVVVV:0xPPPP[:SERIAL|BUS.ADDR]` form; any
    /// other address is resolved as an IPv4 address or hostname.
    pub fn from_address(address: &str, transport: Transport) -> Result<Self> {
        if transport == Transport::Usb {
            let selector = UsbSelector::parse(address).ok_or_else(|| {
                Error::Addr(format!(
                    "invalid USB address '{address}' \
                     (expected usb:0xVVVV:0xPPPP[:SERIAL])"
                ))
            })?;
            return Ok(Self::usb(selector));
        }
        let ip = crate::transport::parse_address(address)?;
        Ok(Self::manual(ip))
    }

    /// The address in `lxi`'s display form: `IP (hostname)` when known.
    pub fn display_address(&self) -> String {
        if let Some(selector) = &self.usb {
            return selector.to_string();
        }
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
        if let Some(selector) = &self.usb {
            return selector.to_string();
        }
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
    /// SCPI over USBTMC on a local USB instrument.
    Usb,
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Transport::Vxi11 => "vxi11",
            Transport::Raw => "raw",
            Transport::Usb => "usb",
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
    /// USB selector, when `transport` is [`Transport::Usb`].
    pub usb: Option<UsbSelector>,
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
            usb: None,
            device_name: default_device_name(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Build a target for a USB (USBTMC) instrument.
    pub fn usb(selector: UsbSelector) -> Self {
        Self {
            address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            transport: Transport::Usb,
            port: None,
            usb: Some(selector),
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
            // USB has no port; the value is never dialed.
            Transport::Usb => 0,
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
            Transport::Usb => None,
        };
        Self {
            address: device.address,
            transport,
            port,
            usb: device.usb.clone(),
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
        assert_eq!(Transport::Usb.to_string(), "usb");
    }

    #[test]
    fn test_usb_selector_parse_roundtrip() {
        let selector = UsbSelector::parse("usb:0x1AB1:0x04CE:DS1ZA123").unwrap();
        assert_eq!(selector.vendor, 0x1AB1);
        assert_eq!(selector.product, 0x04CE);
        assert_eq!(selector.serial.as_deref(), Some("DS1ZA123"));
        assert_eq!(selector.to_string(), "usb:0x1AB1:0x04CE:DS1ZA123");
    }

    #[test]
    fn test_usb_selector_parse_decimal_and_bus_address() {
        // 6833 == 0x1AB1, 1230 == 0x04CE.
        let selector = UsbSelector::parse("usb:6833:1230:1.14").unwrap();
        assert_eq!(selector.vendor, 0x1AB1);
        assert_eq!(selector.product, 0x04CE);
        assert_eq!(selector.bus, Some(1));
        assert_eq!(selector.address, Some(14));
        assert_eq!(selector.to_string(), "usb:0x1AB1:0x04CE:1.14");
    }

    #[test]
    fn test_usb_selector_parse_rejects_garbage() {
        assert!(UsbSelector::parse("192.168.1.50").is_none());
        assert!(UsbSelector::parse("usb:nothex:0x04CE").is_none());
    }

    #[test]
    fn test_target_usb() {
        let target = Target::usb(UsbSelector::new(0x1AB1, 0x04CE));
        assert_eq!(target.transport, Transport::Usb);
        assert_eq!(target.effective_port(), 0);
        assert_eq!(target.usb, Some(UsbSelector::new(0x1AB1, 0x04CE)));
    }

    #[test]
    fn test_device_from_address_usb() {
        let device = Device::from_address("usb:0x1AB1:0x04CE", Transport::Usb).unwrap();
        assert!(device.usb.is_some());
        assert_eq!(device.label(), "usb:0x1AB1:0x04CE");
        // A malformed USB address is rejected rather than silently ignored.
        assert!(Device::from_address("192.168.1.50", Transport::Usb).is_err());
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
