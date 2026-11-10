//! USBTMC transport: SCPI over USB, implementing [`ScpiTransport`].
//!
//! USBTMC wraps SCPI messages in a fixed 12-byte bulk header in each direction.
//! Because it is a `ScpiTransport`, the DS1054Z over USB reuses the entire SCPI
//! path — the driver command surface, screenshots, benchmarking, measurement and
//! the Python API all work unchanged.

use crate::device::Target;
use crate::error::{Error, Result};
use crate::transport::ScpiTransport;

/// Size of the USBTMC bulk message header, in bytes.
const HEADER_LEN: usize = 12;

/// `DEV_DEP_MSG_OUT`: a message from the host to the device.
const MSG_DEV_DEP_MSG_OUT: u8 = 1;

/// `REQUEST_DEV_DEP_MSG_IN`: the host asks the device for a message.
const MSG_REQUEST_DEV_DEP_MSG_IN: u8 = 2;

/// The `EOM` (end of message) transfer attribute bit.
const EOM: u8 = 0x01;

/// How many bytes to request per bulk-in round trip.
///
/// Kept at 64 KiB: some USBTMC instruments, including the Rigol DS1000Z, stall
/// when asked for a larger single transfer. Larger responses are read in
/// several chunks until the `EOM` attribute is set.
const MAX_TRANSFER: usize = 64 * 1024;

const TERMINATOR: u8 = b'\n';

#[cfg(feature = "usb")]
pub use real::UsbTmcTransport;

#[cfg(not(feature = "usb"))]
pub use stub::UsbTmcTransport;

#[cfg(feature = "usb")]
mod real {
    use super::*;
    use crate::usb::backend::UsbHandle;
    use std::time::Duration;

    /// A live USBTMC connection to an instrument.
    pub struct UsbTmcTransport {
        handle: UsbHandle,
        btag: u8,
        closed: bool,
    }

    impl UsbTmcTransport {
        /// Open a USBTMC connection for `target`.
        pub fn connect(target: &Target) -> Result<Self> {
            let selector = target.usb.clone().ok_or_else(|| {
                Error::Usb("USB target has no vendor/product selector".to_string())
            })?;
            let handle = UsbHandle::open(&selector, target.timeout)?;
            Ok(Self {
                handle,
                btag: 0,
                closed: false,
            })
        }

        /// The next bulk message tag, kept in `1..=255`.
        fn next_tag(&mut self) -> u8 {
            self.btag = self.btag.wrapping_add(1);
            if self.btag == 0 {
                self.btag = 1;
            }
            self.btag
        }

        /// Write every byte to the bulk-out endpoint.
        fn write_all(&self, mut data: &[u8]) -> Result<()> {
            while !data.is_empty() {
                match self.handle.write_bulk(data) {
                    Ok(0) => {
                        return Err(Error::UsbTmc("short bulk-out write (0 bytes)".to_string()));
                    }
                    Ok(written) => data = &data[written..],
                    Err(err) => return Err(err),
                }
            }
            Ok(())
        }

        /// Fill `buf` completely from the bulk-in endpoint.
        fn read_exact(&self, buf: &mut [u8]) -> Result<()> {
            let mut filled = 0;
            while filled < buf.len() {
                match self.handle.read_bulk(&mut buf[filled..]) {
                    Ok(0) => {
                        return Err(Error::UsbTmc("short bulk-in read (0 bytes)".to_string()));
                    }
                    Ok(read) => filled += read,
                    Err(err) => return Err(err),
                }
            }
            Ok(())
        }
    }

    impl ScpiTransport for UsbTmcTransport {
        fn send(&mut self, command: &str) -> Result<()> {
            let mut payload = command.as_bytes().to_vec();
            if payload.last() != Some(&TERMINATOR) {
                payload.push(TERMINATOR);
            }

            let tag = self.next_tag();
            let mut message = Vec::with_capacity(HEADER_LEN + payload.len());
            message.push(MSG_DEV_DEP_MSG_OUT);
            message.push(tag);
            message.push(!tag);
            message.push(0);
            message.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            message.push(EOM);
            message.extend_from_slice(&[0, 0, 0]);
            message.extend_from_slice(&payload);

            self.write_all(&message)
        }

        fn receive(&mut self) -> Result<Vec<u8>> {
            let mut data = Vec::new();
            loop {
                let tag = self.next_tag();

                // Ask the device for the next chunk of its response.
                let mut request = [0u8; HEADER_LEN];
                request[0] = MSG_REQUEST_DEV_DEP_MSG_IN;
                request[1] = tag;
                request[2] = !tag;
                request[3] = 0;
                request[4..8].copy_from_slice(&(MAX_TRANSFER as u32).to_le_bytes());
                // No termchar: rely on the EOM attribute instead.
                request[8] = 0;
                self.write_all(&request)?;

                // The device often coalesces the 12-byte header and the payload
                // into a single bulk-in transfer. Reading a 12-byte header first
                // would then overflow, so read into a header+payload buffer and
                // parse the announced length out of it.
                let mut buf = vec![0u8; HEADER_LEN + MAX_TRANSFER];
                let mut filled = 0usize;
                let mut payload_len = None;
                let mut zero_reads = 0u32;
                loop {
                    let read = self.handle.read_bulk(&mut buf[filled..])?;
                    if read == 0 {
                        // A USBTMC device may emit a zero-length packet while it
                        // is still preparing a large response (the DS1000Z does
                        // this while rendering a screenshot). Treat it as "not
                        // ready yet" and retry briefly before giving up.
                        if filled >= HEADER_LEN {
                            break;
                        }
                        zero_reads += 1;
                        if zero_reads > 200 {
                            return Err(Error::UsbTmc(
                                "no data after 200 zero-length reads".to_string(),
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    filled += read;
                    zero_reads = 0;
                    if payload_len.is_none() && filled >= HEADER_LEN {
                        if buf[0] != MSG_REQUEST_DEV_DEP_MSG_IN {
                            return Err(Error::UsbTmc(format!(
                                "unexpected bulk-in message id {}",
                                buf[0]
                            )));
                        }
                        let declared =
                            u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
                        payload_len = Some(declared);
                    }
                    if let Some(len) = payload_len
                        && filled >= HEADER_LEN + len
                    {
                        break;
                    }
                }

                let len = payload_len.unwrap_or(0);
                let attributes = buf[8];
                data.extend_from_slice(&buf[HEADER_LEN..HEADER_LEN + len]);

                if attributes & EOM != 0 || len == 0 {
                    break;
                }
            }

            // Strip the transport terminator, but never eat a payload byte of a
            // definite-length binary block (a screenshot, say).
            crate::transport::strip_response_terminator(&mut data);
            Ok(data)
        }

        fn close(&mut self) -> Result<()> {
            // The handle releases the interface and reattaches the kernel driver
            // on drop; there is no USBTMC link to tear down.
            self.closed = true;
            Ok(())
        }

        fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
            self.handle.set_timeout(timeout);
            Ok(())
        }

        fn read_status_byte(&mut self) -> Result<u8> {
            self.handle.read_status_byte()
        }

        fn device_clear(&mut self) -> Result<()> {
            self.handle.device_clear()
        }

        fn device_trigger(&mut self) -> Result<()> {
            // USBTMC has no separate trigger endpoint; `*TRG` is the portable
            // SCPI equivalent and is what instruments implement.
            self.send("*TRG")
        }

        fn description(&self) -> String {
            format!("USBTMC (interface {})", self.handle.interface())
        }
    }
}

#[cfg(not(feature = "usb"))]
mod stub {
    use super::*;

    /// A placeholder that reports USB support as not compiled in.
    pub struct UsbTmcTransport {
        _private: (),
    }

    impl UsbTmcTransport {
        /// Always fails: the `usb` feature was not enabled at build time.
        pub fn connect(_target: &Target) -> Result<Self> {
            Err(Error::Unsupported(
                "USBTMC support was not compiled in; rebuild with the `usb` feature".to_string(),
            ))
        }
    }

    impl ScpiTransport for UsbTmcTransport {
        fn send(&mut self, _command: &str) -> Result<()> {
            Err(Error::Unsupported(
                "USBTMC support was not compiled in".to_string(),
            ))
        }

        fn receive(&mut self) -> Result<Vec<u8>> {
            Err(Error::Unsupported(
                "USBTMC support was not compiled in".to_string(),
            ))
        }

        fn close(&mut self) -> Result<()> {
            Ok(())
        }

        fn description(&self) -> String {
            "USBTMC (unavailable; `usb` feature disabled)".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `DEV_DEP_MSG_OUT` header and verify its fields.
    #[test]
    fn test_bulk_out_header_layout() {
        let payload = b":RUN\n";
        let tag = 7u8;
        let mut message = vec![MSG_DEV_DEP_MSG_OUT, tag, !tag, 0];
        message.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        message.push(EOM);
        message.extend_from_slice(&[0, 0, 0]);

        assert_eq!(message.len(), HEADER_LEN);
        assert_eq!(message[0], MSG_DEV_DEP_MSG_OUT);
        assert_eq!(message[1], tag);
        assert_eq!(message[2], !tag);
        assert_eq!(
            u32::from_le_bytes([message[4], message[5], message[6], message[7]]) as usize,
            payload.len()
        );
        assert_eq!(message[8] & EOM, EOM);
    }

    #[test]
    fn test_terminator_is_newline() {
        assert_eq!(TERMINATOR, b'\n');
    }
}
