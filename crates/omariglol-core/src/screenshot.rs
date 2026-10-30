//! Screenshot capture, mirroring `lxi screenshot`.
//!
//! Most LXI instruments return their screen as an IEEE-488.2 definite-length
//! binary block in response to a vendor-specific query. The block parser is
//! generic; the per-vendor commands live in [`PROFILES`] and are selected by
//! matching the instrument's `*IDN?` response.

use std::net::IpAddr;
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::device::{Device, Target, Transport};
use crate::error::{Error, Result};
use crate::transport::raw::DEFAULT_RAW_PORT;
use crate::transport::{self, ScpiTransport};

/// Image format of a captured screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Bmp,
    Jpeg,
    Unknown,
}

impl ImageFormat {
    /// Guess the format from the file's magic bytes.
    pub fn sniff(data: &[u8]) -> ImageFormat {
        const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        if data.starts_with(PNG_MAGIC) {
            return ImageFormat::Png;
        }
        if data.starts_with(b"BM") {
            return ImageFormat::Bmp;
        }
        if data.starts_with(&[0xff, 0xd8, 0xff]) {
            return ImageFormat::Jpeg;
        }
        ImageFormat::Unknown
    }

    /// The conventional file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Unknown => "bin",
        }
    }

    /// The MIME type used in a `data:` URL.
    pub fn mime(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Unknown => "application/octet-stream",
        }
    }
}

/// A supported instrument family's screenshot recipe.
#[derive(Debug, Clone, Copy)]
pub struct ScreenshotProfile {
    /// Friendly family name shown in the UI.
    pub name: &'static str,
    /// `*IDN?` prefixes that select this profile (upper-cased comparison).
    pub idn_prefixes: &'static [&'static str],
    /// Commands sent before the capture query.
    pub setup: &'static [&'static str],
    /// The query whose response is the image block.
    pub command: &'static str,
    /// What the instrument is expected to return.
    pub format: ImageFormat,
    /// The vendor's raw SCPI socket port, used when capturing over raw TCP.
    pub raw_port: u16,
}

/// Rigol's raw SCPI socket port. Rigol does not use the LXI-standard 5025.
pub const RIGOL_RAW_PORT: u16 = 5555;

/// Screenshot recipes, most specific first.
///
/// Populating this table is the main piece of 0.1.0 work that needs real
/// hardware; each entry should come with a captured sample.
pub const PROFILES: &[ScreenshotProfile] = &[
    ScreenshotProfile {
        name: "Rigol DS1000Z",
        idn_prefixes: &[
            "RIGOL TECHNOLOGIES,DS1",
            "RIGOL TECHNOLOGIES,MSO1",
            "RIGOL TECHNOLOGIES,DS1000Z",
        ],
        setup: &[":STOP"],
        // The DS1000Z can emit PNG directly, which is ~20x smaller than the
        // default BMP (52 KiB vs 1.1 MiB) and therefore far quicker to pull
        // over the wire and to embed in the UI.
        command: ":DISP:DATA? ON,OFF,PNG",
        format: ImageFormat::Png,
        raw_port: RIGOL_RAW_PORT,
    },
    ScreenshotProfile {
        name: "Rigol DS2000",
        idn_prefixes: &[
            "RIGOL TECHNOLOGIES,DS2",
            "RIGOL TECHNOLOGIES,MSO2",
            "RIGOL TECHNOLOGIES,DS2000",
        ],
        setup: &[":STOP"],
        command: ":DISP:DATA?",
        format: ImageFormat::Bmp,
        raw_port: RIGOL_RAW_PORT,
    },
    ScreenshotProfile {
        name: "Rigol DS4000",
        idn_prefixes: &[
            "RIGOL TECHNOLOGIES,DS4",
            "RIGOL TECHNOLOGIES,MSO4",
            "RIGOL TECHNOLOGIES,DS4000",
        ],
        setup: &[":STOP"],
        command: ":DISP:DATA?",
        format: ImageFormat::Bmp,
        raw_port: RIGOL_RAW_PORT,
    },
    ScreenshotProfile {
        name: "Keysight InfiniiVision",
        idn_prefixes: &[
            "KEYSIGHT TECHNOLOGIES,DSO",
            "KEYSIGHT TECHNOLOGIES,MSO",
            "AGILENT TECHNOLOGIES,DSO",
            "AGILENT TECHNOLOGIES,MSO",
        ],
        setup: &[":STOP"],
        command: ":DISP:DATA? PNG",
        format: ImageFormat::Png,
        raw_port: DEFAULT_RAW_PORT,
    },
    ScreenshotProfile {
        name: "Rohde & Schwarz",
        idn_prefixes: &["ROHDE&SCHWARZ", "ROHDE & SCHWARZ"],
        setup: &[":SYST:DISP:UPD OFF"],
        command: ":HCOP:DEV:LANG PNG;:HCOP:DATA?",
        format: ImageFormat::Png,
        raw_port: DEFAULT_RAW_PORT,
    },
];

/// Pick the profile matching an `*IDN?` string.
pub fn profile_for_idn(idn: &str) -> Option<&'static ScreenshotProfile> {
    let upper = idn.trim().to_ascii_uppercase();
    PROFILES.iter().find(|profile| {
        profile
            .idn_prefixes
            .iter()
            .any(|prefix| upper.starts_with(prefix))
    })
}

/// The distinct raw SCPI ports the known profiles use, in probe order.
///
/// The vendor is only known after `*IDN?`, so the port cannot be chosen from
/// the profile up front; instead every known port is tried in turn.
pub fn raw_port_candidates() -> Vec<u16> {
    let mut ports = Vec::new();
    for profile in PROFILES {
        if !ports.contains(&profile.raw_port) {
            ports.push(profile.raw_port);
        }
    }
    ports
}

/// Open the default transport for image acquisition.
///
/// Raw TCP is the default because it streams the image at TCP speed, whereas
/// VXI-11 caps each read at the instrument's `maxRecvSize` (often a single
/// Ethernet MTU), which makes large captures crawl. Each known vendor raw port
/// is tried in turn; a refused connection fails immediately, so probing is
/// cheap.
pub fn open_default_transport(
    address: IpAddr,
    timeout: Duration,
) -> Result<Box<dyn ScpiTransport>> {
    let mut last_err = None;
    for port in raw_port_candidates() {
        let target = Target::new(address, Transport::Raw)
            .with_port(port)
            .with_timeout(timeout);
        match transport::open(&target) {
            Ok(transport) => return Ok(transport),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| Error::Other("no raw SCPI port candidates".to_string())))
}

/// A captured screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshot {
    /// Raw image bytes exactly as the instrument sent them.
    #[serde(skip)]
    pub data: Vec<u8>,
    pub format: ImageFormat,
    /// The `*IDN?` string of the instrument.
    pub idn: String,
    /// Which profile produced this, or `"custom"` when the user overrode the
    /// command.
    pub profile: String,
}

impl Screenshot {
    /// Base64 encoding of the image, for the Tauri IPC bridge.
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.data)
    }

    /// A `data:` URL suitable for direct embedding in the frontend.
    pub fn to_data_url(&self) -> String {
        format!("data:{};base64,{}", self.format.mime(), self.to_base64())
    }
}

/// Capture a screenshot from an open session.
///
/// `custom_command` overrides the profile's capture query, which is how the GUI
/// supports instruments that are not in [`PROFILES`].
pub fn capture(
    transport: &mut dyn ScpiTransport,
    idn: &str,
    custom_command: Option<&str>,
) -> Result<Screenshot> {
    let profile = profile_for_idn(idn);

    let (command, profile_name, expected_format) = match custom_command {
        Some(cmd) => (cmd, "custom".to_string(), ImageFormat::Unknown),
        None => match profile {
            Some(p) => (p.command, p.name.to_string(), p.format),
            None => {
                return Err(Error::ScreenshotUnsupported(format!(
                    "no screenshot profile matches {idn:?}; pass a custom command"
                )));
            }
        },
    };

    // Run the profile's setup commands, if any. A custom command has no setup.
    if custom_command.is_none() {
        if let Some(p) = profile {
            for setup in p.setup {
                transport.send(setup)?;
            }
        }
    }

    let raw = transport.query(command)?;
    let data = parse_ieee_block(&raw).unwrap_or(raw);

    // Prefer what the bytes actually are; fall back to the profile's promise.
    let sniffed = ImageFormat::sniff(&data);
    let format = if sniffed == ImageFormat::Unknown {
        expected_format
    } else {
        sniffed
    };

    Ok(Screenshot {
        data,
        format,
        idn: idn.to_string(),
        profile: profile_name,
    })
}

/// Capture a screenshot from an address over the fastest available transport.
///
/// The instrument is identified first, so the correct vendor profile and raw
/// SCPI port are selected; raw TCP is preferred over VXI-11 because it streams
/// the image at TCP speed.
pub fn capture_from_address(
    address: IpAddr,
    timeout: Duration,
    custom_command: Option<&str>,
) -> Result<Screenshot> {
    let mut transport = open_default_transport(address, timeout)?;
    let idn = String::from_utf8_lossy(&transport.query("*IDN?")?)
        .trim()
        .to_string();
    capture(transport.as_mut(), &idn, custom_command)
}

/// Open a transport for a workspace instrument, honouring USB.
///
/// Network instruments keep the fast raw-TCP probing behaviour of
/// [`open_default_transport`]; a USB instrument is opened over USBTMC using its
/// selector. Keeping this decision here means every caller (CLI snapshots, the
/// desktop app and the shell plugin) reaches USB instruments the same way.
pub fn open_transport_for_device(
    device: &Device,
    transport_kind: Transport,
    port: Option<u16>,
    timeout: Duration,
) -> Result<Box<dyn ScpiTransport>> {
    match transport_kind {
        // A USB instrument is addressed by its selector, never by IP.
        Transport::Usb => {
            let mut target = Target::from_device(device, Transport::Usb).with_timeout(timeout);
            if let Some(port) = port {
                target = target.with_port(port);
            }
            transport::open(&target)
        }
        // VXI-11 is explicit: honour it instead of probing raw TCP, which may
        // be disabled while the portmapper and VXI-11 server still work.
        Transport::Vxi11 => {
            let mut target = Target::from_device(device, Transport::Vxi11).with_timeout(timeout);
            if let Some(port) = port {
                target = target.with_port(port);
            }
            transport::open(&target)
        }
        // Raw is usually discovered by probing the vendor ports; an explicit
        // port pins the connection to the one that answered.
        Transport::Raw => {
            if port.is_some() || device.raw_port.is_some() {
                let mut target = Target::from_device(device, Transport::Raw).with_timeout(timeout);
                if let Some(port) = port {
                    target = target.with_port(port);
                }
                transport::open(&target)
            } else {
                open_default_transport(device.address, timeout)
            }
        }
    }
}

/// Capture a screenshot from a workspace instrument, over USB when needed.
pub fn capture_from_device(
    device: &Device,
    transport_kind: Transport,
    port: Option<u16>,
    timeout: Duration,
    custom_command: Option<&str>,
) -> Result<Screenshot> {
    let mut transport = open_transport_for_device(device, transport_kind, port, timeout)?;
    let idn = String::from_utf8_lossy(&transport.query("*IDN?")?)
        .trim()
        .to_string();
    capture(transport.as_mut(), &idn, custom_command)
}

/// Extract the payload from an IEEE-488.2 definite-length block.
///
/// Accepts `#<n><len><data>` and `#0<data>` (indefinite, newline-terminated).
/// Data that is not a block at all is returned unchanged, so instruments that
/// reply with a bare image work too.
pub fn parse_ieee_block(data: &[u8]) -> Option<Vec<u8>> {
    if data.first() != Some(&b'#') {
        // Not a block: pass the bytes through untouched.
        return Some(data.to_vec());
    }

    let digit = *data.get(1)?;
    if !digit.is_ascii_digit() {
        // A `#` that is not a block header is malformed.
        return None;
    }
    let n = (digit - b'0') as usize;

    if n == 0 {
        // Indefinite length: the payload runs to the end, minus a trailing
        // newline terminator if the instrument added one.
        let mut payload = data.get(2..)?.to_vec();
        if payload.last() == Some(&b'\n') {
            payload.pop();
            if payload.last() == Some(&b'\r') {
                payload.pop();
            }
        }
        return Some(payload);
    }

    let len_start: usize = 2;
    let len_end = len_start.checked_add(n)?;
    let len_digits = data.get(len_start..len_end)?;
    let len_str = std::str::from_utf8(len_digits).ok()?;
    let len: usize = len_str.parse().ok()?;

    let payload_start = len_end;
    let payload_end = payload_start.checked_add(len)?;
    let payload = data.get(payload_start..payload_end)?;
    Some(payload.to_vec())
}

/// Total length of the complete IEEE block in `data`, if the header is present.
///
/// Used to know when a streamed image has fully arrived.
pub(crate) fn declared_block_len(data: &[u8]) -> Option<usize> {
    if data.first() != Some(&b'#') {
        return None;
    }
    let digit = *data.get(1)?;
    if !digit.is_ascii_digit() {
        return None;
    }
    let n = (digit - b'0') as usize;
    if n == 0 {
        // Indefinite length: no declared total.
        return None;
    }
    let len_start: usize = 2;
    let len_end = len_start.checked_add(n)?;
    let len_digits = data.get(len_start..len_end)?;
    let len_str = std::str::from_utf8(len_digits).ok()?;
    let len: usize = len_str.parse().ok()?;
    len_end.checked_add(len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::collections::VecDeque;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x01];
    const BMP: &[u8] = b"BM\x00\x00\x00\x00";
    const JPEG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 0x00];

    /// A real capture from a Rigol DS1054Z (800x480 PNG), used to prove the
    /// block parser and format sniffer handle genuine instrument output.
    const DS1054Z_SAMPLE: &[u8] = include_bytes!("../tests/fixtures/rigol-ds1054z.png");

    #[test]
    fn test_image_format_sniff_png() {
        assert_eq!(ImageFormat::sniff(PNG), ImageFormat::Png);
    }

    #[test]
    fn test_image_format_sniff_bmp() {
        assert_eq!(ImageFormat::sniff(BMP), ImageFormat::Bmp);
    }

    #[test]
    fn test_image_format_sniff_jpeg() {
        assert_eq!(ImageFormat::sniff(JPEG), ImageFormat::Jpeg);
    }

    #[test]
    fn test_image_format_sniff_unknown() {
        assert_eq!(ImageFormat::sniff(b"not an image"), ImageFormat::Unknown);
        assert_eq!(ImageFormat::sniff(&[]), ImageFormat::Unknown);
    }

    #[test]
    fn test_image_format_extension() {
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Bmp.extension(), "bmp");
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormat::Unknown.extension(), "bin");
    }

    #[test]
    fn test_profile_for_idn() {
        assert_eq!(
            profile_for_idn("RIGOL TECHNOLOGIES,DS1054Z,DS1ZA123,1.0")
                .unwrap()
                .name,
            "Rigol DS1000Z"
        );
        assert_eq!(
            profile_for_idn("rigol technologies,mso2072a,0,1.0")
                .unwrap()
                .name,
            "Rigol DS2000"
        );
        assert_eq!(
            profile_for_idn("RIGOL TECHNOLOGIES,DS4024,0,1.0")
                .unwrap()
                .name,
            "Rigol DS4000"
        );
        assert_eq!(
            profile_for_idn("KEYSIGHT TECHNOLOGIES,DSOX1102G,CN123,1.0")
                .unwrap()
                .name,
            "Keysight InfiniiVision"
        );
        assert_eq!(
            profile_for_idn("Rohde&Schwarz,RTB2004,123,1.0")
                .unwrap()
                .name,
            "Rohde & Schwarz"
        );
        assert!(profile_for_idn("Some Other Vendor,Model,0,1.0").is_none());
    }

    #[test]
    fn test_every_profile_matches_its_prefix() {
        // Every profile must be reachable through `profile_for_idn` using its
        // own first prefix, so adding a profile without a matching test fails.
        for profile in PROFILES {
            let idn = format!("{},MODEL,0,1.0", profile.idn_prefixes[0]);
            let matched = profile_for_idn(&idn).unwrap_or_else(|| {
                panic!("no profile matched prefix {:?}", profile.idn_prefixes[0])
            });
            assert_eq!(matched.name, profile.name);
        }
    }

    #[test]
    fn test_captured_sample_parses() {
        // The instrument sends the image as a definite-length block: `#9`, nine
        // length digits, then the payload. The captured sample must survive the
        // round trip and still sniff as a PNG.
        let mut block = format!("#9{:09}", DS1054Z_SAMPLE.len()).into_bytes();
        block.extend_from_slice(DS1054Z_SAMPLE);

        let parsed = parse_ieee_block(&block).expect("captured block should parse");
        assert_eq!(parsed, DS1054Z_SAMPLE);
        assert_eq!(ImageFormat::sniff(&parsed), ImageFormat::Png);
        assert_eq!(declared_block_len(&block), Some(block.len()));
    }

    #[test]
    fn test_raw_port_candidates() {
        let ports = raw_port_candidates();
        // Both the Rigol port and the LXI-standard port are offered.
        assert!(ports.contains(&RIGOL_RAW_PORT));
        assert!(ports.contains(&DEFAULT_RAW_PORT));
        // Deduplicated.
        let mut sorted = ports.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ports.len());
    }

    #[test]
    fn test_parse_ieee_block_definite() {
        let block = b"#15hello";
        assert_eq!(parse_ieee_block(block).unwrap(), b"hello");
    }

    #[test]
    fn test_parse_ieee_block_definite_multidigit_len() {
        let payload = vec![0xabu8; 300];
        let mut block = b"#3300".to_vec();
        block.extend_from_slice(&payload);
        assert_eq!(parse_ieee_block(&block).unwrap(), payload);
    }

    #[test]
    fn test_parse_ieee_block_indefinite() {
        assert_eq!(parse_ieee_block(b"#0hello\n").unwrap(), b"hello");
        assert_eq!(parse_ieee_block(b"#0hello\r\n").unwrap(), b"hello");
        assert_eq!(parse_ieee_block(b"#0hello").unwrap(), b"hello");
    }

    #[test]
    fn test_parse_ieee_block_non_block_passthrough() {
        assert_eq!(parse_ieee_block(b"plain text").unwrap(), b"plain text");
        assert_eq!(parse_ieee_block(PNG).unwrap(), PNG);
    }

    #[test]
    fn test_parse_ieee_block_truncated_is_none() {
        // Declares 5 bytes but only 3 follow.
        assert!(parse_ieee_block(b"#15hel").is_none());
        // Header cut off before the length digits.
        assert!(parse_ieee_block(b"#3").is_none());
        // `#` followed by a non-digit is malformed.
        assert!(parse_ieee_block(b"#xhello").is_none());
    }

    #[test]
    fn test_declared_block_len() {
        assert_eq!(declared_block_len(b"#15hello"), Some(8));
        // n=3, length digits "300", payload 300 bytes: 2 + 3 + 300.
        assert_eq!(declared_block_len(b"#3300"), Some(305));
        // Indefinite blocks have no declared total.
        assert_eq!(declared_block_len(b"#0hello\n"), None);
        // Not a block.
        assert_eq!(declared_block_len(b"plain"), None);
        // Incomplete header.
        assert_eq!(declared_block_len(b"#3"), None);
    }

    #[test]
    fn test_screenshot_to_base64() {
        let shot = Screenshot {
            data: b"hello".to_vec(),
            format: ImageFormat::Png,
            idn: "x".to_string(),
            profile: "custom".to_string(),
        };
        assert_eq!(shot.to_base64(), "aGVsbG8=");
    }

    #[test]
    fn test_screenshot_to_data_url() {
        let shot = Screenshot {
            data: b"hello".to_vec(),
            format: ImageFormat::Png,
            idn: "x".to_string(),
            profile: "custom".to_string(),
        };
        assert_eq!(shot.to_data_url(), "data:image/png;base64,aGVsbG8=");
    }

    /// A transport that replays queued responses and records what was sent.
    struct MockTransport {
        sent: Vec<String>,
        responses: VecDeque<Vec<u8>>,
    }

    impl MockTransport {
        fn new(responses: Vec<Vec<u8>>) -> Self {
            MockTransport {
                sent: Vec::new(),
                responses: responses.into_iter().collect(),
            }
        }
    }

    impl ScpiTransport for MockTransport {
        fn send(&mut self, command: &str) -> Result<()> {
            self.sent.push(command.to_string());
            Ok(())
        }

        fn receive(&mut self) -> Result<Vec<u8>> {
            self.responses
                .pop_front()
                .ok_or_else(|| Error::Other("no response queued".to_string()))
        }

        fn close(&mut self) -> Result<()> {
            Ok(())
        }

        fn description(&self) -> String {
            "mock".to_string()
        }
    }

    #[test]
    fn test_capture_with_profile() {
        let mut block = b"#210".to_vec();
        block.extend_from_slice(PNG);
        let mut transport = MockTransport::new(vec![block]);
        let shot = capture(&mut transport, "RIGOL TECHNOLOGIES,DS1054Z,0,1.0", None).unwrap();
        assert_eq!(shot.profile, "Rigol DS1000Z");
        assert_eq!(shot.format, ImageFormat::Png);
        assert_eq!(shot.data, PNG);
        // Setup command then the capture query.
        assert_eq!(
            transport.sent,
            vec![":STOP".to_string(), ":DISP:DATA? ON,OFF,PNG".to_string()]
        );
    }

    #[test]
    fn test_capture_with_custom_command() {
        let mut block = b"#210".to_vec();
        block.extend_from_slice(PNG);
        let mut transport = MockTransport::new(vec![block]);
        let shot = capture(&mut transport, "Unknown Vendor,Model,0,1.0", Some(":FOO?")).unwrap();
        assert_eq!(shot.profile, "custom");
        assert_eq!(shot.format, ImageFormat::Png);
        assert_eq!(shot.data, PNG);
        assert_eq!(transport.sent, vec![":FOO?".to_string()]);
    }

    #[test]
    fn test_capture_unknown_without_custom_errors() {
        let mut transport = MockTransport::new(vec![]);
        let err = capture(&mut transport, "Unknown Vendor,Model,0,1.0", None).unwrap_err();
        assert!(matches!(err, Error::ScreenshotUnsupported(_)));
    }
}
