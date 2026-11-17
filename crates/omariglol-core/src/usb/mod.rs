//! USB device abstraction and protocol drivers.
//!
//! The USB layer is deliberately two tiers:
//!
//! 1. A **device abstraction** ([`UsbId`], [`UsbDeviceInfo`], enumeration) that
//!    is backend-agnostic.
//! 2. **Protocol drivers** on top — currently [`usbtmc`], which is a SCPI
//!    transport, so the DS1054Z over USB reuses the entire SCPI path.
//!
//! The actual libusb binding lives behind the `usb` cargo feature. Without it
//! the module still compiles — enumeration returns nothing and
//! [`usbtmc::UsbTmcTransport::connect`] reports the capability as unsupported —
//! so the core builds on machines without libusb-1.0 installed.

pub mod usbtmc;

#[cfg(feature = "usb")]
pub mod backend;

use serde::{Deserialize, Serialize};

pub use crate::device::UsbSelector;

/// The USB interface class for USBTMC devices.
pub const USBTMC_CLASS: u8 = 0xFE;

/// The USB interface subclass for USBTMC devices.
pub const USBTMC_SUBCLASS: u8 = 0x03;

/// A USB vendor/product identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbId {
    /// USB vendor id.
    pub vendor: u16,
    /// USB product id.
    pub product: u16,
}

impl UsbId {
    /// Build an id from the vendor/product pair.
    pub const fn new(vendor: u16, product: u16) -> Self {
        Self { vendor, product }
    }
}

/// An enumerated USB device, independent of any backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDeviceInfo {
    /// Vendor/product identity.
    pub id: UsbId,
    /// Bus number.
    pub bus: u8,
    /// Device address on the bus.
    pub address: u8,
    /// Serial number string, when readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Manufacturer string, when readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Product string, when readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// The USBTMC interface number, when the device has one.
    pub interface_number: u8,
    /// True when the device exposes a USBTMC interface.
    pub is_usbtmc: bool,
    /// True when the device is a first-class non-SCPI instrument recognised by
    /// its USB vendor/product id — the NI USB-6009 DAQ, say. Such a device is
    /// listed and labelled by discovery but speaks no SCPI, so it has no
    /// `*IDN?` command surface.
    pub is_daq: bool,
}

impl UsbDeviceInfo {
    /// Turn this device into a selector pinned to its serial (or bus/device).
    pub fn selector(&self) -> UsbSelector {
        let mut selector = UsbSelector::new(self.id.vendor, self.id.product);
        if let Some(serial) = &self.serial {
            selector.serial = Some(serial.clone());
        } else {
            selector.bus = Some(self.bus);
            selector.address = Some(self.address);
        }
        selector
    }

    /// True when `selector` would match this device during enumeration.
    pub fn matches(&self, selector: &UsbSelector) -> bool {
        if self.id.vendor != selector.vendor || self.id.product != selector.product {
            return false;
        }
        if let Some(serial) = &selector.serial {
            return self.serial.as_deref() == Some(serial.as_str());
        }
        if let (Some(bus), Some(address)) = (selector.bus, selector.address) {
            return self.bus == bus && self.address == address;
        }
        true
    }

    /// A human label: a recognised device's profile name, then the product
    /// string, then the bare vendor/product ids.
    pub fn label(&self) -> String {
        crate::driver::usb_device_label(self.id.vendor, self.id.product, self.product.as_deref())
    }

    /// The protocol driver this device is reachable through.
    ///
    /// `usbtmc` for a SCPI-over-USB instrument, `daq` for a recognised
    /// non-SCPI USB device, and `-` for anything else. This is what the USB
    /// list and discovery surface show.
    pub fn driver_kind(&self) -> &'static str {
        if self.is_usbtmc {
            "usbtmc"
        } else if self.is_daq {
            "daq"
        } else {
            "-"
        }
    }
}

/// Enumerate attached USB devices.
///
/// Without the `usb` feature this is always empty; the caller can still show an
/// empty list rather than branching on the feature.
pub fn enumerate() -> Vec<UsbDeviceInfo> {
    #[cfg(feature = "usb")]
    {
        backend::enumerate()
    }
    #[cfg(not(feature = "usb"))]
    {
        Vec::new()
    }
}

/// True when this build includes the USB backend.
pub fn is_enabled() -> bool {
    cfg!(feature = "usb")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(vendor: u16, product: u16, serial: Option<&str>) -> UsbDeviceInfo {
        UsbDeviceInfo {
            id: UsbId::new(vendor, product),
            bus: 1,
            address: 7,
            serial: serial.map(str::to_string),
            manufacturer: None,
            product: None,
            interface_number: 0,
            is_usbtmc: true,
            is_daq: false,
        }
    }

    fn daq_info() -> UsbDeviceInfo {
        UsbDeviceInfo {
            id: UsbId::new(0x3923, 0x717b),
            bus: 1,
            address: 90,
            serial: Some("0x12345678".to_string()),
            manufacturer: Some("National Instruments".to_string()),
            product: Some("USB-6009".to_string()),
            interface_number: 0,
            is_usbtmc: false,
            is_daq: true,
        }
    }

    #[test]
    fn test_daq_label_and_driver_kind() {
        let daq = daq_info();
        assert_eq!(daq.label(), "NI USB-6009");
        assert_eq!(daq.driver_kind(), "daq");
        assert_eq!(info(0x1AB1, 0x04CE, None).driver_kind(), "usbtmc");
        // A plain, unrecognised device shows no driver.
        let plain = UsbDeviceInfo {
            is_usbtmc: false,
            is_daq: false,
            product: None,
            ..info(0x1234, 0x5678, None)
        };
        assert_eq!(plain.driver_kind(), "-");
        assert_eq!(plain.label(), "1234:5678");
    }

    #[test]
    fn test_matches_by_ids() {
        let device = info(0x1AB1, 0x04CE, None);
        assert!(device.matches(&UsbSelector::new(0x1AB1, 0x04CE)));
        assert!(!device.matches(&UsbSelector::new(0x1AB1, 0x0001)));
    }

    #[test]
    fn test_matches_by_serial() {
        let device = info(0x1AB1, 0x04CE, Some("DS1ZA123"));
        let matching = UsbSelector::new(0x1AB1, 0x04CE).with_serial("DS1ZA123");
        let other = UsbSelector::new(0x1AB1, 0x04CE).with_serial("NOPE");
        assert!(device.matches(&matching));
        assert!(!device.matches(&other));
    }

    #[test]
    fn test_matches_by_bus_address() {
        let device = info(0x1AB1, 0x04CE, None);
        let mut selector = UsbSelector::new(0x1AB1, 0x04CE);
        selector.bus = Some(1);
        selector.address = Some(7);
        assert!(device.matches(&selector));
        selector.address = Some(8);
        assert!(!device.matches(&selector));
    }

    #[test]
    fn test_selector_prefers_serial() {
        let device = info(0x1AB1, 0x04CE, Some("DS1ZA123"));
        let selector = device.selector();
        assert_eq!(selector.serial.as_deref(), Some("DS1ZA123"));
        assert!(selector.bus.is_none());
    }
}
