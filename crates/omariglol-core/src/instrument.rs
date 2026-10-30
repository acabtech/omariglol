//! The workspace instrument model.
//!
//! A [`Device`](crate::device::Device) is what discovery finds; an
//! [`Instrument`] is what a workspace *owns*. It wraps a device with the
//! per-instrument configuration, connection state and capabilities the
//! multi-instrument platform needs (concepts.md §4.1).
//!
//! This is deliberately UI-agnostic and blocking-free: it is pure data plus
//! small helpers, so it can be persisted, cloned and tested without a network.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::device::{Device, Transport};

/// A stable identifier for an instrument within a workspace.
///
/// Ids are opaque strings. They are derived from the device's address and
/// transport when an instrument is added, and never change afterwards, so a
/// preset or rule that references an instrument keeps working across
/// reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InstrumentId(pub String);

impl InstrumentId {
    /// Build an id from a device, preferring a stable address+transport key.
    ///
    /// A USB instrument is keyed by its selector rather than its (unspecified)
    /// IP address, so two devices attached at once cannot collapse onto the same
    /// `0.0.0.0` id.
    pub fn from_device(device: &Device, transport: Transport) -> Self {
        if let Some(selector) = &device.usb {
            return InstrumentId(selector.to_string());
        }
        InstrumentId(format!("{}:{}", transport, device.address))
    }

    /// Build an id from an arbitrary string, for manual or imported entries.
    pub fn new(id: impl Into<String>) -> Self {
        InstrumentId(id.into())
    }

    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The connection lifecycle state of an instrument (concepts.md §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstrumentState {
    /// In the workspace but not connected.
    Disconnected,
    /// A connect is in flight.
    Connecting,
    /// Connected and responsive.
    Connected,
    /// The last operation failed; a retry is pending.
    Error,
    /// Unreachable after the retry window elapsed.
    Offline,
}

impl InstrumentState {
    /// True when the instrument can accept commands.
    pub fn is_connected(&self) -> bool {
        matches!(self, InstrumentState::Connected)
    }

    /// True when the instrument is in a failure state.
    pub fn is_failed(&self) -> bool {
        matches!(self, InstrumentState::Error | InstrumentState::Offline)
    }
}

impl fmt::Display for InstrumentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            InstrumentState::Disconnected => "disconnected",
            InstrumentState::Connecting => "connecting",
            InstrumentState::Connected => "connected",
            InstrumentState::Error => "error",
            InstrumentState::Offline => "offline",
        };
        f.write_str(name)
    }
}

/// Per-instrument configuration, persisted with the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentConfig {
    /// Transport to open the instrument with.
    pub transport: Transport,
    /// Explicit port override, or `None` for the transport default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Socket timeout, in seconds.
    pub timeout_secs: u64,
    /// A named vendor profile, when the user pinned one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_profile: Option<String>,
    /// User-defined SCPI command aliases (`"freq"` -> `":MEAS:FREQ?"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aliases: BTreeMap<String, String>,
}

impl InstrumentConfig {
    /// A configuration for a device, choosing the transport it advertises.
    pub fn for_device(device: &Device) -> Self {
        let transport = if device.usb.is_some() {
            Transport::Usb
        } else if device.vxi11_port.is_some() {
            Transport::Vxi11
        } else {
            Transport::Raw
        };
        let port = match transport {
            Transport::Vxi11 => device.vxi11_port,
            Transport::Raw => device.raw_port,
            Transport::Usb => None,
        };
        InstrumentConfig {
            transport,
            port,
            timeout_secs: 3,
            vendor_profile: None,
            aliases: BTreeMap::new(),
        }
    }

    /// The socket timeout as a [`Duration`].
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

impl Default for InstrumentConfig {
    fn default() -> Self {
        InstrumentConfig {
            transport: Transport::Vxi11,
            port: None,
            timeout_secs: 3,
            vendor_profile: None,
            aliases: BTreeMap::new(),
        }
    }
}

/// A workspace member: a device plus its configuration, state and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instrument {
    /// Stable id within the workspace.
    pub id: InstrumentId,
    /// The discovered or manually entered device.
    pub device: Device,
    /// Per-instrument configuration.
    pub config: InstrumentConfig,
    /// Current connection state.
    pub state: InstrumentState,
    /// The last `*IDN?` response, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idn: Option<String>,
    /// The last error message, when the instrument is in a failure state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Millisecond timestamp of the last activity, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_ms: Option<u64>,
    /// SCPI command groups the instrument is believed to support.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

impl Instrument {
    /// Wrap a device as a workspace instrument.
    pub fn from_device(device: Device) -> Self {
        let config = InstrumentConfig::for_device(&device);
        let id = InstrumentId::from_device(&device, config.transport);
        Instrument {
            id,
            device,
            config,
            state: InstrumentState::Disconnected,
            idn: None,
            last_error: None,
            last_activity_ms: None,
            capabilities: Vec::new(),
        }
    }

    /// A short human label: the mDNS name, hostname or address.
    pub fn label(&self) -> String {
        self.device.label()
    }

    /// The instrument type inferred from `*IDN?`, when known.
    pub fn instrument_type(&self) -> Option<crate::driver::InstrumentType> {
        self.idn.as_deref().map(crate::driver::classify_idn)
    }

    /// Mark the instrument connected, recording its identity.
    pub fn mark_connected(&mut self, idn: String, now_ms: u64) {
        self.state = InstrumentState::Connected;
        self.idn = Some(idn);
        self.last_error = None;
        self.last_activity_ms = Some(now_ms);
    }

    /// Mark the instrument disconnected, keeping its configuration.
    pub fn mark_disconnected(&mut self) {
        self.state = InstrumentState::Disconnected;
    }

    /// Mark the instrument failed, recording the reason.
    pub fn mark_error(&mut self, message: impl Into<String>, now_ms: u64) {
        self.state = InstrumentState::Error;
        self.last_error = Some(message.into());
        self.last_activity_ms = Some(now_ms);
    }

    /// Mark the instrument offline after the retry window elapsed.
    pub fn mark_offline(&mut self, now_ms: u64) {
        self.state = InstrumentState::Offline;
        self.last_activity_ms = Some(now_ms);
    }

    /// Record activity without changing the state.
    pub fn touch(&mut self, now_ms: u64) {
        self.last_activity_ms = Some(now_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn device(addr: &str) -> Device {
        Device::manual(addr.parse::<IpAddr>().unwrap())
    }

    #[test]
    fn test_instrument_id_from_device() {
        let id = InstrumentId::from_device(&device("192.168.1.50"), Transport::Vxi11);
        assert_eq!(id.as_str(), "vxi11:192.168.1.50");
        assert_eq!(id.to_string(), "vxi11:192.168.1.50");
    }

    #[test]
    fn test_instrument_id_usb_is_selector_not_unspecified_ip() {
        let a = Device::usb(crate::device::UsbSelector::new(0x1AB1, 0x04CE).with_serial("A"));
        let b = Device::usb(crate::device::UsbSelector::new(0x1AB1, 0x04CE).with_serial("B"));
        let id_a = InstrumentId::from_device(&a, Transport::Usb);
        let id_b = InstrumentId::from_device(&b, Transport::Usb);
        assert_eq!(id_a.as_str(), "usb:0x1AB1:0x04CE:A");
        assert_ne!(id_a, id_b);
        assert!(!id_a.as_str().contains("0.0.0.0"));
    }

    #[test]
    fn test_instrument_from_device_prefers_vxi11() {
        let mut dev = device("192.168.1.50");
        dev.vxi11_port = Some(1024);
        dev.raw_port = Some(5025);
        let instrument = Instrument::from_device(dev);
        assert_eq!(instrument.config.transport, Transport::Vxi11);
        assert_eq!(instrument.config.port, Some(1024));
        assert_eq!(instrument.state, InstrumentState::Disconnected);
        assert!(instrument.idn.is_none());
    }

    #[test]
    fn test_instrument_from_device_falls_back_to_raw() {
        let mut dev = device("192.168.1.51");
        dev.raw_port = Some(5025);
        let instrument = Instrument::from_device(dev);
        assert_eq!(instrument.config.transport, Transport::Raw);
        assert_eq!(instrument.config.port, Some(5025));
    }

    #[test]
    fn test_instrument_lifecycle() {
        let mut instrument = Instrument::from_device(device("192.168.1.50"));
        instrument.mark_connected("RIGOL TECHNOLOGIES,DS1054Z".to_string(), 1000);
        assert!(instrument.state.is_connected());
        assert_eq!(
            instrument.instrument_type(),
            Some(crate::driver::InstrumentType::Oscilloscope)
        );
        assert_eq!(instrument.last_activity_ms, Some(1000));

        instrument.mark_error("timed out", 2000);
        assert!(instrument.state.is_failed());
        assert_eq!(instrument.last_error.as_deref(), Some("timed out"));

        instrument.mark_offline(3000);
        assert_eq!(instrument.state, InstrumentState::Offline);

        instrument.mark_disconnected();
        assert_eq!(instrument.state, InstrumentState::Disconnected);
        // Configuration and identity survive a disconnect.
        assert!(instrument.idn.is_some());
    }

    #[test]
    fn test_instrument_state_display() {
        assert_eq!(InstrumentState::Connected.to_string(), "connected");
        assert_eq!(InstrumentState::Offline.to_string(), "offline");
    }

    #[test]
    fn test_instrument_config_timeout() {
        let config = InstrumentConfig {
            timeout_secs: 7,
            ..Default::default()
        };
        assert_eq!(config.timeout(), Duration::from_secs(7));
    }
}
