//! Screenshot capture, mirroring `lxi screenshot`.
//!
//! Most LXI instruments return their screen as an IEEE-488.2 definite-length
//! binary block in response to a vendor-specific query. The block parser is
//! generic; the per-vendor commands live in [`PROFILES`] and are selected by
//! matching the instrument's `*IDN?` response.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::transport::ScpiTransport;

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
        let _ = data;
        todo!("check PNG/BMP/JPEG magic bytes")
    }

    /// The conventional file extension for this format.
    pub fn extension(&self) -> &'static str {
        let _ = self;
        todo!("map format to extension")
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
}

/// Screenshot recipes, most specific first.
///
/// Populating this table is the main piece of 0.1.0 work that needs real
/// hardware; each entry should come with a captured sample.
pub const PROFILES: &[ScreenshotProfile] = &[];

/// Pick the profile matching an `*IDN?` string.
pub fn profile_for_idn(idn: &str) -> Option<&'static ScreenshotProfile> {
    let _ = idn;
    todo!("match the IDN against the profile prefix table")
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
        let _ = self;
        todo!("base64-encode the image")
    }

    /// A `data:` URL suitable for direct embedding in the frontend.
    pub fn to_data_url(&self) -> String {
        let _ = self;
        todo!("build a data: URL from the format and bytes")
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
    let _ = (transport, idn, custom_command);
    todo!("select a profile, run setup, read the block and parse it")
}

/// Extract the payload from an IEEE-488.2 definite-length block.
///
/// Accepts `#<n><len><data>` and `#0<data>` (indefinite, newline-terminated).
/// Data that is not a block at all is returned unchanged, so instruments that
/// reply with a bare image work too.
pub fn parse_ieee_block(data: &[u8]) -> Option<Vec<u8>> {
    let _ = data;
    todo!("parse #<n><len><payload> and #0<payload>")
}

/// Total length of the complete IEEE block in `data`, if the header is present.
///
/// Used to know when a streamed image has fully arrived.
pub(crate) fn declared_block_len(data: &[u8]) -> Option<usize> {
    let _ = data;
    todo!("return the declared total block length")
}
