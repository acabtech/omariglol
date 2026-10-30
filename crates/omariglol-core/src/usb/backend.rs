//! The `rusb` (libusb-1.0) backend for the USB layer.
//!
//! This module is compiled only with the `usb` cargo feature so the core still
//! builds on machines without libusb. It hides every `rusb` type behind
//! [`UsbHandle`]; the USBTMC driver above it only sees bulk and control
//! transfers.

use std::time::Duration;

use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};

use crate::device::UsbSelector;
use crate::error::{Error, Result};

use super::{USBTMC_CLASS, USBTMC_SUBCLASS, UsbDeviceInfo, UsbId};

// USBTMC class-specific control requests (USB Test & Measurement Class spec).
const USBTMC_READ_STATUS_BYTE: u8 = 128;
const USBTMC_INITIATE_CLEAR: u8 = 5;
const USBTMC_CHECK_CLEAR_STATUS: u8 = 6;

/// Device-to-host, class, interface.
const CTL_IN_CLASS_INTERFACE: u8 = 0xA1;

fn map_err(err: rusb::Error) -> Error {
    Error::Usb(err.to_string())
}

/// Find the USBTMC interface and its bulk endpoints in a device's active config.
///
/// Returns `(interface_number, bulk_in_endpoint, bulk_out_endpoint)`.
fn find_usbtmc_interface(
    device: &rusb::Device<Context>,
) -> Option<(u8, u8, u8)> {
    let config = device.active_config_descriptor().ok()?;
    for interface in config.interfaces() {
        for descriptor in interface.descriptors() {
            if descriptor.class_code() != USBTMC_CLASS
                || descriptor.sub_class_code() != USBTMC_SUBCLASS
            {
                continue;
            }
            let mut bulk_in = None;
            let mut bulk_out = None;
            for endpoint in descriptor.endpoint_descriptors() {
                if endpoint.transfer_type() != TransferType::Bulk {
                    continue;
                }
                match endpoint.direction() {
                    Direction::In => bulk_in = Some(endpoint.address()),
                    Direction::Out => bulk_out = Some(endpoint.address()),
                }
            }
            if let (Some(bulk_in), Some(bulk_out)) = (bulk_in, bulk_out) {
                return Some((descriptor.interface_number(), bulk_in, bulk_out));
            }
        }
    }
    None
}

/// A claimed, open USB device ready for bulk and control transfers.
pub struct UsbHandle {
    handle: DeviceHandle<Context>,
    interface: u8,
    bulk_in: u8,
    bulk_out: u8,
    timeout: Duration,
    detached_kernel_driver: bool,
}

impl UsbHandle {
    /// Open the first device matching `selector` and claim its USBTMC interface.
    pub fn open(selector: &UsbSelector, timeout: Duration) -> Result<Self> {
        let context = Context::new().map_err(map_err)?;
        let devices = context.devices().map_err(map_err)?;

        for device in devices.iter() {
            let descriptor = match device.device_descriptor() {
                Ok(descriptor) => descriptor,
                Err(_) => continue,
            };
            if descriptor.vendor_id() != selector.vendor
                || descriptor.product_id() != selector.product
            {
                continue;
            }
            let Some((interface, bulk_in, bulk_out)) = find_usbtmc_interface(&device) else {
                continue;
            };

            // Bus/address filter, applied before opening where possible.
            if let (Some(bus), Some(address)) = (selector.bus, selector.address) {
                if device.bus_number() != bus || device.address() != address {
                    continue;
                }
            }

            let handle = match device.open() {
                Ok(handle) => handle,
                Err(err) => {
                    return Err(Error::Usb(format!(
                        "cannot open USB instrument {selector}: {err}. On Linux the \
                         in-kernel `usbtmc` driver may own the device; detach it or \
                         install the udev rule shipped with omariglol"
                    )));
                }
            };

            // Serial filter, which needs the open handle to read the string.
            if let Some(serial) = &selector.serial {
                let found = handle
                    .read_serial_number_string_ascii(&descriptor)
                    .ok();
                if found.as_deref() != Some(serial.as_str()) {
                    continue;
                }
            }

            let mut detached_kernel_driver = false;
            if handle
                .kernel_driver_active(interface)
                .map_err(map_err)?
            {
                handle.detach_kernel_driver(interface).map_err(map_err)?;
                detached_kernel_driver = true;
            }
            handle.claim_interface(interface).map_err(|err| {
                Error::Usb(format!(
                    "cannot claim USBTMC interface {interface}: {err}"
                ))
            })?;

            return Ok(Self {
                handle,
                interface,
                bulk_in,
                bulk_out,
                timeout,
                detached_kernel_driver,
            });
        }

        Err(Error::Usb(format!(
            "no USBTMC instrument matching {selector} was found"
        )))
    }

    /// Retune the transfer timeout.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// The claimed USBTMC interface number.
    pub fn interface(&self) -> u8 {
        self.interface
    }

    /// Write to the bulk-out endpoint, returning the number of bytes written.
    pub fn write_bulk(&self, data: &[u8]) -> Result<usize> {
        self.handle
            .write_bulk(self.bulk_out, data, self.timeout)
            .map_err(map_err)
    }

    /// Read from the bulk-in endpoint, returning the number of bytes read.
    pub fn read_bulk(&self, buf: &mut [u8]) -> Result<usize> {
        self.handle
            .read_bulk(self.bulk_in, buf, self.timeout)
            .map_err(map_err)
    }

    /// Issue a class interface control read.
    pub fn control_read(
        &self,
        request: u8,
        value: u16,
        index: u16,
        buf: &mut [u8],
    ) -> Result<usize> {
        self.handle
            .read_control(
                CTL_IN_CLASS_INTERFACE,
                request,
                value,
                index,
                buf,
                self.timeout,
            )
            .map_err(map_err)
    }

    /// Read the USBTMC status byte.
    pub fn read_status_byte(&self) -> Result<u8> {
        let mut buf = [0u8; 2];
        let read = self.control_read(
            USBTMC_READ_STATUS_BYTE,
            0,
            self.interface as u16,
            &mut buf,
        )?;
        if read == 0 {
            return Err(Error::UsbTmc("empty status-byte response".to_string()));
        }
        Ok(buf[0])
    }

    /// Clear the device: `INITIATE_CLEAR` then `CHECK_CLEAR_STATUS`.
    pub fn device_clear(&self) -> Result<()> {
        let mut buf = [0u8; 1];
        self.control_read(
            USBTMC_INITIATE_CLEAR,
            0,
            self.interface as u16,
            &mut buf,
        )?;
        let mut status = [0u8; 2];
        self.control_read(
            USBTMC_CHECK_CLEAR_STATUS,
            0,
            self.interface as u16,
            &mut status,
        )?;
        // Bit 0 is CLEAR_STATUS_PENDING; anything else means a stall happened.
        if status[0] & 0x01 != 0 {
            return Err(Error::UsbTmc(
                "device clear is still pending".to_string(),
            ));
        }
        Ok(())
    }
}

impl Drop for UsbHandle {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(self.interface);
        if self.detached_kernel_driver {
            // Best effort: hand the interface back to the kernel driver.
            let _ = self.handle.attach_kernel_driver(self.interface);
        }
    }
}

/// Enumerate attached USB devices, annotating which expose a USBTMC interface.
pub fn enumerate() -> Vec<UsbDeviceInfo> {
    let context = match Context::new() {
        Ok(context) => context,
        Err(_) => return Vec::new(),
    };
    let devices = match context.devices() {
        Ok(devices) => devices,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for device in devices.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(descriptor) => descriptor,
            Err(_) => continue,
        };
        let usbtmc = find_usbtmc_interface(&device);

        let (serial, manufacturer, product) = match device.open() {
            Ok(handle) => (
                handle.read_serial_number_string_ascii(&descriptor).ok(),
                handle.read_manufacturer_string_ascii(&descriptor).ok(),
                handle.read_product_string_ascii(&descriptor).ok(),
            ),
            Err(_) => (None, None, None),
        };

        out.push(UsbDeviceInfo {
            id: UsbId::new(descriptor.vendor_id(), descriptor.product_id()),
            bus: device.bus_number(),
            address: device.address(),
            serial,
            manufacturer,
            product,
            interface_number: usbtmc.map(|(interface, _, _)| interface).unwrap_or(0),
            is_usbtmc: usbtmc.is_some(),
        });
    }
    out
}
