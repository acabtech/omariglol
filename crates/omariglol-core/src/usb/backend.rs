//! The `rusb` (libusb-1.0) backend for the USB layer.
//!
//! This module is compiled only with the `usb` cargo feature so the core still
//! builds on machines without libusb. It hides every `rusb` type behind
//! [`UsbHandle`]; the USBTMC driver above it only sees bulk and control
//! transfers.

use std::path::PathBuf;
use std::time::{Duration, Instant};

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

/// Cypress/Anchor FX2 vendor request to access CPUCS/RAM (used for RAM download).
const FX2_REQ_RAM: u8 = 0xA0;
/// FX2 CPUCS register address.
const FX2_CPUCS: u16 = 0xE600;

const NANODLA_BOOTLOADER_VID: u16 = 0x1d50;
const NANODLA_BOOTLOADER_PID: u16 = 0x608c;
const NANODLA_USBTMC_VID: u16 = 0x0912;
const NANODLA_USBTMC_PID: u16 = 0x1100;

fn map_err(err: rusb::Error) -> Error {
    Error::Usb(err.to_string())
}

/// Keep printable text and drop control/replacement noise from USB strings.
fn sanitize_usb_string(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_control() && *c != '\u{FFFD}')
        .collect();
    cleaned.trim().to_string()
}

fn read_usb_strings(
    handle: &DeviceHandle<Context>,
    descriptor: &rusb::DeviceDescriptor,
) -> (Option<String>, Option<String>, Option<String>) {
    let timeout = Duration::from_millis(250);
    let langs = handle.read_languages(timeout).ok().unwrap_or_default();
    let lang = langs
        .iter()
        .find(|l| l.lang_id() == 0x0409)
        .copied()
        .or_else(|| langs.into_iter().next());

    let (mut serial, mut manufacturer, mut product) = if let Some(lang) = lang {
        (
            handle
                .read_serial_number_string(lang, descriptor, timeout)
                .ok(),
            handle
                .read_manufacturer_string(lang, descriptor, timeout)
                .ok(),
            handle.read_product_string(lang, descriptor, timeout).ok(),
        )
    } else {
        (None, None, None)
    };

    if serial.is_none() {
        serial = handle.read_serial_number_string_ascii(descriptor).ok();
    }
    if manufacturer.is_none() {
        manufacturer = handle.read_manufacturer_string_ascii(descriptor).ok();
    }
    if product.is_none() {
        product = handle.read_product_string_ascii(descriptor).ok();
    }

    (
        serial.map(|s| sanitize_usb_string(&s)).filter(|s| !s.is_empty()),
        manufacturer
            .map(|s| sanitize_usb_string(&s))
            .filter(|s| !s.is_empty()),
        product.map(|s| sanitize_usb_string(&s)).filter(|s| !s.is_empty()),
    )
}

/// Identify a nanoDLA device and normalize its display strings.
fn identify_nanodla(
    vendor: u16,
    product: u16,
    manufacturer: Option<String>,
    mut model: Option<String>,
) -> (Option<String>, Option<String>) {
    if vendor == NANODLA_BOOTLOADER_VID && product == NANODLA_BOOTLOADER_PID {
        return (
            Some(manufacturer.unwrap_or_else(|| "MuseLab".to_string())),
            Some("nanoDLA (FX2 bootloader)".to_string()),
        );
    }
    if vendor == NANODLA_USBTMC_VID && product == NANODLA_USBTMC_PID {
        let man = manufacturer.unwrap_or_else(|| "MuseLab".to_string());
        let prod = match model.take() {
            Some(p) if !p.is_empty() && p != "USB Device" => p,
            _ => "nanoDLA USBTMC".to_string(),
        };
        let prod = if prod.to_ascii_lowercase().contains("nanodla") {
            prod
        } else {
            "nanoDLA USBTMC".to_string()
        };
        return (Some(man), Some(prod));
    }
    (manufacturer, model)
}

/// Parse an Intel HEX (.ihx) image into contiguous (address, data) chunks.
fn parse_ihex(data: &str) -> std::result::Result<Vec<(u32, Vec<u8>)>, String> {
    let mut seg: u32 = 0;
    let mut mem: std::collections::BTreeMap<u32, u8> = std::collections::BTreeMap::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with(':') {
            continue;
        }
        let bytes = (2..line.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&line[i..i + 2], 16).map_err(|e| e.to_string()))
            .collect::<std::result::Result<Vec<u8>, _>>()?;
        if bytes.len() < 5 {
            continue;
        }
        let n = bytes[0] as usize;
        let addr = ((bytes[1] as u32) << 8) | bytes[2] as u32;
        let rec = bytes[3];
        if bytes.len() < 4 + n {
            continue;
        }
        let payload = &bytes[4..4 + n];
        match rec {
            0x00 => {
                let a = seg + addr;
                for (i, b) in payload.iter().enumerate() {
                    mem.insert(a + i as u32, *b);
                }
            }
            0x01 => break,
            0x02 if payload.len() >= 2 => {
                seg = (((payload[0] as u32) << 8) | payload[1] as u32) << 4;
            }
            0x04 if payload.len() >= 2 => {
                seg = (((payload[0] as u32) << 8) | payload[1] as u32) << 16;
            }
            _ => {}
        }
    }
    if mem.is_empty() {
        return Ok(Vec::new());
    }
    let mut chunks: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut iter = mem.into_iter();
    let (mut start, b0) = iter.next().unwrap();
    let mut cur = vec![b0];
    let mut prev = start;
    for (a, b) in iter {
        if a == prev + 1 {
            cur.push(b);
        } else {
            chunks.push((start, std::mem::take(&mut cur)));
            start = a;
            cur.push(b);
        }
        prev = a;
    }
    chunks.push((start, cur));
    Ok(chunks)
}

fn nanodla_firmware_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("OMARIGLOL_NANODLA_USBTMC_FW") {
        out.push(PathBuf::from(p));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        out.push(home.join("projects/nanoDLA-USBTMC/firmware/build/usbtmc-usbtmc/nanodla-usbtmc-usbtmc.ihx"));
        out.push(home.join("projects/nanoDLA-USBTMC/dist/nanodla-usbtmc-usbtmc.ihx"));
    }
    out
}

/// Try to RAM-load the nanoDLA USBTMC firmware into an FX2 bootloader device.
///
/// Returns true if a load was attempted (successfully sent).
fn try_load_nanodla_firmware(device: &rusb::Device<Context>) -> bool {
    let fw_path = nanodla_firmware_candidates().into_iter().find(|p| p.is_file());
    let Some(fw_path) = fw_path else { return false };
    let Ok(ihx) = std::fs::read_to_string(&fw_path) else { return false };
    let Ok(chunks) = parse_ihex(&ihx) else { return false };
    if chunks.is_empty() {
        return false;
    }

    let Ok(handle) = device.open() else { return false };
    let timeout = Duration::from_millis(1000);

    // Reset CPU (hold in reset), download, release.
    let _ = handle.write_control(0x40, FX2_REQ_RAM, FX2_CPUCS, 0, &[1], timeout);
    for (addr, data) in &chunks {
        let mut off = 0usize;
        while off < data.len() {
            let end = std::cmp::min(off + 4096, data.len());
            let part = &data[off..end];
            let a = addr + off as u32;
            if a > 0xFFFF {
                break;
            }
            if handle
                .write_control(0x40, FX2_REQ_RAM, a as u16, 0, part, timeout)
                .is_err()
            {
                let _ = handle.write_control(0x40, FX2_REQ_RAM, FX2_CPUCS, 0, &[0], timeout);
                return false;
            }
            off = end;
        }
    }
    let _ = handle.write_control(0x40, FX2_REQ_RAM, FX2_CPUCS, 0, &[0], timeout);
    true
}

/// Find the USBTMC interface and its bulk endpoints in a device's active config.
///
/// Returns `(interface_number, bulk_in_endpoint, bulk_out_endpoint)`.
fn find_usbtmc_interface(device: &rusb::Device<Context>) -> Option<(u8, u8, u8)> {
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
                let found = read_usb_strings(&handle, &descriptor)
                    .0
                    .or_else(|| handle.read_serial_number_string_ascii(&descriptor).ok())
                    .map(|s| sanitize_usb_string(&s));
                if found.as_deref() != Some(serial.as_str()) {
                    continue;
                }
            }

            let mut detached_kernel_driver = false;
            if handle.kernel_driver_active(interface).map_err(map_err)? {
                handle.detach_kernel_driver(interface).map_err(map_err)?;
                detached_kernel_driver = true;
            }
            handle.claim_interface(interface).map_err(|err| {
                Error::Usb(format!("cannot claim USBTMC interface {interface}: {err}"))
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
        let read =
            self.control_read(USBTMC_READ_STATUS_BYTE, 0, self.interface as u16, &mut buf)?;
        if read == 0 {
            return Err(Error::UsbTmc("empty status-byte response".to_string()));
        }
        Ok(buf[0])
    }

    /// Clear the device: `INITIATE_CLEAR` then `CHECK_CLEAR_STATUS`.
    pub fn device_clear(&self) -> Result<()> {
        let mut buf = [0u8; 1];
        self.control_read(USBTMC_INITIATE_CLEAR, 0, self.interface as u16, &mut buf)?;
        let mut status = [0u8; 2];
        self.control_read(
            USBTMC_CHECK_CLEAR_STATUS,
            0,
            self.interface as u16,
            &mut status,
        )?;
        // Bit 0 is CLEAR_STATUS_PENDING; anything else means a stall happened.
        if status[0] & 0x01 != 0 {
            return Err(Error::UsbTmc("device clear is still pending".to_string()));
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

fn enumerate_once(context: &Context) -> Vec<UsbDeviceInfo> {
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
            Ok(handle) => read_usb_strings(&handle, &descriptor),
            Err(_) => (None, None, None),
        };
        let (manufacturer, product) = identify_nanodla(
            descriptor.vendor_id(),
            descriptor.product_id(),
            manufacturer,
            product,
        );

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

/// Enumerate attached USB devices, annotating which expose a USBTMC interface.
///
/// If a MuseLab nanoDLA is found in FX2 bootloader mode (`1d50:608c`, OpenMoko
/// Fx2lafw) and a custom USBTMC RAM firmware image is available, it is loaded
/// automatically and the bus is re-enumerated so the device appears as a USBTMC
/// instrument (`0912:1100`).
pub fn enumerate() -> Vec<UsbDeviceInfo> {
    let context = match Context::new() {
        Ok(context) => context,
        Err(_) => return Vec::new(),
    };

    let mut devices = enumerate_once(&context);

    // Auto-load custom USBTMC firmware into nanoDLA bootloader devices.
    if let Ok(list) = context.devices() {
        let mut loaded = false;
        for device in list.iter() {
            let Ok(desc) = device.device_descriptor() else { continue };
            if desc.vendor_id() == NANODLA_BOOTLOADER_VID
                && desc.product_id() == NANODLA_BOOTLOADER_PID
                && try_load_nanodla_firmware(&device)
            {
                loaded = true;
            }
        }
        if loaded {
            // Give the device time to re-enumerate with the new firmware.
            let deadline = Instant::now() + Duration::from_millis(2500);
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(250));
                devices = enumerate_once(&context);
                if devices.iter().any(|d| {
                    d.id.vendor == NANODLA_USBTMC_VID && d.id.product == NANODLA_USBTMC_PID
                }) {
                    break;
                }
            }
        }
    }

    devices
}
