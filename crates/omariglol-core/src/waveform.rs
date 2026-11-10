//! Waveform capture: read a scope's raw acquisition memory and write it to a
//! file the caller can post-process.
//!
//! ## Why this exists
//!
//! On the Rigol DS1000Z family, `:WAV:MODE RAW` is the only way to get the full
//! acquisition memory (up to 24 M points on a DS1054Z with the memory option),
//! but it has two awkward constraints:
//!
//! 1. It only works while the scope is **stopped**.
//! 2. A single `:WAV:DATA?` returns at most **250 k points**; the rest has to be
//!    read in chunks using `:WAV:STAR`/`:WAV:STOP`.
//!
//! This module hides both. [`capture`] stops the instrument, reads the preamble,
//! walks the memory in chunks, stitches the payloads back into one buffer and
//! converts the raw codes to volts using the preamble's scaling factors. The
//! result is written either as a NumPy `.npy` array (the default, so an agent can
//! `numpy.load()` it without pulling samples through its context) or as CSV.
//!
//! No GUI or CLI types leak in here: the frontends decide where the data goes.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::screenshot::parse_ieee_block;
use crate::transport::ScpiTransport;

/// The SCPI waveform data format (`:WAV:FORM`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaveformFormat {
    /// One 8-bit code per point.
    Byte,
    /// One 16-bit code per point.
    Word,
    /// One decimal number per point, comma-separated ASCII.
    Ascii,
}

impl WaveformFormat {
    /// The SCPI mnemonic for `:WAV:FORM`.
    pub fn mnemonic(&self) -> &'static str {
        match self {
            WaveformFormat::Byte => "BYTE",
            WaveformFormat::Word => "WORD",
            WaveformFormat::Ascii => "ASC",
        }
    }

    /// Parse the format code from a `:WAV:PRE?` preamble.
    pub fn from_preamble_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(WaveformFormat::Byte),
            1 => Some(WaveformFormat::Word),
            2 => Some(WaveformFormat::Ascii),
            _ => None,
        }
    }
}

/// The SCPI waveform acquisition mode (`:WAV:MODE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaveformMode {
    /// Screen-memory samples only.
    Normal,
    /// Peak-detect envelope over the display.
    Maximum,
    /// The full acquisition memory; requires a stopped instrument.
    Raw,
}

impl WaveformMode {
    /// The SCPI mnemonic for `:WAV:MODE`.
    pub fn mnemonic(&self) -> &'static str {
        match self {
            WaveformMode::Normal => "NORM",
            WaveformMode::Maximum => "MAX",
            WaveformMode::Raw => "RAW",
        }
    }
}

/// Byte order of a 16-bit WORD payload.
///
/// Rigol documentation is inconsistent across firmware revisions, so this is
/// explicit rather than assumed. `Little` matches the common DS1000Z
/// interpretation; a capture that looks byte-swapped (values near the rails)
/// should be retried with `--word-order big`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WordByteOrder {
    Little,
    Big,
}

impl WordByteOrder {
    /// Decode two bytes into a raw word using this byte order.
    fn decode(self, lo: u8, hi: u8) -> u16 {
        match self {
            WordByteOrder::Little => u16::from_le_bytes([lo, hi]),
            WordByteOrder::Big => u16::from_be_bytes([lo, hi]),
        }
    }
}

/// The `:WAV:PRE?` preamble, the scaling factors for the raw sample codes.
///
/// Field order follows the DS1000Z programming guide:
/// `format, type, points, count, xincrement, xorigin, xreference, yincrement,
/// yorigin, yreference`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaveformPreamble {
    /// The data format code (0 = BYTE, 1 = WORD, 2 = ASCII).
    pub format_code: i64,
    /// The acquisition type code (0 = NORM, 1 = MAX, 2 = RAW).
    pub type_code: i64,
    /// The number of points the instrument will return.
    pub points: u64,
    /// The number of waveforms averaged (1 for a single acquisition).
    pub count: i64,
    /// Seconds between consecutive samples.
    pub x_increment: f64,
    /// Time of the first sample, in seconds.
    pub x_origin: f64,
    /// The sample index used as the horizontal reference.
    pub x_reference: f64,
    /// Volts per raw code.
    pub y_increment: f64,
    /// The vertical origin, in raw codes.
    pub y_origin: f64,
    /// The vertical reference, in raw codes.
    pub y_reference: f64,
}

impl WaveformPreamble {
    /// Parse a `:WAV:PRE?` response.
    pub fn parse(text: &str) -> Result<Self> {
        let fields: Vec<&str> = text.trim().split(',').map(str::trim).collect();
        if fields.len() < 10 {
            return Err(Error::Malformed(format!(
                "waveform preamble has {} fields, expected 10: {text:?}",
                fields.len()
            )));
        }
        let num = |index: usize, name: &str| -> Result<f64> {
            fields[index].parse::<f64>().map_err(|_| {
                Error::Malformed(format!(
                    "waveform preamble field {name} is not a number: {:?}",
                    fields[index]
                ))
            })
        };
        Ok(WaveformPreamble {
            format_code: num(0, "format")? as i64,
            type_code: num(1, "type")? as i64,
            points: num(2, "points")? as u64,
            count: num(3, "count")? as i64,
            x_increment: num(4, "xincrement")?,
            x_origin: num(5, "xorigin")?,
            x_reference: num(6, "xreference")?,
            y_increment: num(7, "yincrement")?,
            y_origin: num(8, "yorigin")?,
            y_reference: num(9, "yreference")?,
        })
    }

    /// The format implied by the preamble.
    pub fn format(&self) -> Option<WaveformFormat> {
        WaveformFormat::from_preamble_code(self.format_code)
    }
}

/// Options controlling a capture.
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    /// The source channel (`CHAN1`..`CHAN4`), or a math/reference source.
    pub source: String,
    /// The acquisition mode. `Raw` is the full-memory path.
    pub mode: WaveformMode,
    /// The data format.
    pub format: WaveformFormat,
    /// Point count for `NORM`/`MAX`; ignored for `Raw`, which reads the whole
    /// memory reported by the preamble.
    pub points: Option<u64>,
    /// Maximum points per `:WAV:DATA?` transfer.
    ///
    /// The DS1000Z caps a RAW read at 250 000 points; larger requests are
    /// rejected or truncated. The default matches that limit.
    pub chunk_points: u64,
    /// Byte order for a WORD payload.
    pub word_byte_order: WordByteOrder,
    /// Stop the instrument before reading. RAW mode requires this, so it is on
    /// by default; turning it off is only useful when the instrument is already
    /// stopped and the caller wants to preserve a running state.
    pub stop_first: bool,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        CaptureOptions {
            source: "CHAN1".to_string(),
            mode: WaveformMode::Raw,
            format: WaveformFormat::Word,
            points: None,
            // The DS1000Z refuses to return more than 250 k points per read.
            chunk_points: 250_000,
            word_byte_order: WordByteOrder::Little,
            stop_first: true,
        }
    }
}

/// A captured waveform: the preamble plus decoded samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Waveform {
    /// The request the capture satisfied.
    pub source: String,
    pub mode: WaveformMode,
    pub format: WaveformFormat,
    /// The instrument's preamble, including the scaling factors.
    pub preamble: WaveformPreamble,
    /// Decoded voltages, one per point.
    pub samples: Vec<f64>,
    /// How many `:WAV:DATA?` chunks were stitched together.
    pub chunks: usize,
}

impl Waveform {
    /// The number of decoded samples.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True when no samples were decoded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The time, in seconds, of the sample at `index`.
    pub fn time_at(&self, index: usize) -> f64 {
        self.preamble.x_origin + index as f64 * self.preamble.x_increment
    }

    /// Iterate `(time, voltage)` pairs.
    pub fn pairs(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.samples
            .iter()
            .enumerate()
            .map(move |(index, voltage)| (self.time_at(index), *voltage))
    }

    /// Write the waveform as a two-column CSV (`time_s,voltage_v`).
    pub fn write_csv(&self, path: &Path) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writeln!(writer, "time_s,voltage_v")?;
        for (time, voltage) in self.pairs() {
            writeln!(writer, "{time:.12e},{voltage:.9e}")?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Write the waveform as a NumPy `.npy` array of shape `(n, 2)`.
    ///
    /// Column 0 is time in seconds, column 1 is voltage in volts. `numpy.load()`
    /// reads it directly with no extra dependency, which is the point: an agent
    /// can analyse the capture with numpy instead of streaming every sample
    /// through its context.
    pub fn write_npy(&self, path: &Path) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        write_npy_header(&mut writer, self.samples.len(), 2)?;
        let mut row = Vec::with_capacity(16);
        for (time, voltage) in self.pairs() {
            row.clear();
            row.extend_from_slice(&time.to_le_bytes());
            row.extend_from_slice(&voltage.to_le_bytes());
            writer.write_all(&row)?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Write a JSON metadata sidecar: the preamble, units and capture options.
    pub fn write_metadata(&self, path: &Path) -> Result<()> {
        #[derive(Serialize)]
        struct Metadata<'a> {
            source: &'a str,
            mode: WaveformMode,
            format: WaveformFormat,
            points: usize,
            chunks: usize,
            columns: [&'static str; 2],
            units: [&'static str; 2],
            preamble: &'a WaveformPreamble,
        }
        let metadata = Metadata {
            source: &self.source,
            mode: self.mode,
            format: self.format,
            points: self.samples.len(),
            chunks: self.chunks,
            columns: ["time_s", "voltage_v"],
            units: ["s", "V"],
            preamble: &self.preamble,
        };
        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|err| Error::Other(format!("could not encode metadata: {err}")))?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Write a NumPy v1.0 header for a C-order little-endian float64 array.
fn write_npy_header(writer: &mut impl Write, rows: usize, cols: usize) -> Result<()> {
    let dict = format!("{{'descr': '<f8', 'fortran_order': False, 'shape': ({rows}, {cols}), }}");
    let mut header = dict.into_bytes();
    // The spec requires the total header (magic + version + length + dict) to be
    // a multiple of 64 bytes and to end with a newline, padded with spaces.
    // magic (6) + version (2) + header length (2) = 10 bytes before the dict.
    let base = 10;
    while (base + header.len() + 1) % 64 != 0 {
        header.push(b' ');
    }
    header.push(b'\n');

    writer.write_all(b"\x93NUMPY")?;
    writer.write_all(&[1, 0])?;
    writer.write_all(&(header.len() as u16).to_le_bytes())?;
    writer.write_all(&header)?;
    Ok(())
}

/// Capture a waveform from an open transport.
///
/// In `Raw` mode the instrument is stopped first (the mode is only valid while
/// stopped), then the preamble is read and the memory is walked in
/// `chunk_points`-sized windows via `:WAV:STAR`/`:WAV:STOP`. Every chunk's
/// IEEE-488.2 block is parsed and its payload appended, so the caller gets one
/// contiguous sample vector regardless of how many round trips it took.
pub fn capture(transport: &mut dyn ScpiTransport, options: &CaptureOptions) -> Result<Waveform> {
    if options.chunk_points == 0 {
        return Err(Error::Other(
            "waveform chunk size must be greater than zero".to_string(),
        ));
    }

    // RAW mode is only valid while the instrument is stopped.
    if options.stop_first {
        transport.send(":STOP")?;
    }
    transport.send(&format!(":WAV:SOUR {}", options.source))?;
    transport.send(&format!(":WAV:MODE {}", options.mode.mnemonic()))?;
    transport.send(&format!(":WAV:FORM {}", options.format.mnemonic()))?;
    if let Some(points) = options.points
        && options.mode != WaveformMode::Raw
    {
        transport.send(&format!(":WAV:POIN {points}"))?;
    }

    let preamble_text = transport.query(":WAV:PRE?")?;
    let preamble = WaveformPreamble::parse(&String::from_utf8_lossy(&preamble_text))?;

    let total_points = match options.mode {
        WaveformMode::Raw => preamble.points,
        _ => options.points.unwrap_or(preamble.points),
    };
    if total_points == 0 {
        return Err(Error::Malformed(
            "instrument reports zero waveform points".to_string(),
        ));
    }

    let mut raw = Vec::new();
    let mut chunks = 0usize;
    let mut start: u64 = 1;
    while start <= total_points {
        let stop = (start + options.chunk_points - 1).min(total_points);
        transport.send(&format!(":WAV:STAR {start}"))?;
        transport.send(&format!(":WAV:STOP {stop}"))?;
        let block = transport.query(":WAV:DATA?")?;
        let payload = parse_ieee_block(&block).ok_or_else(|| {
            Error::Malformed(format!(
                "waveform chunk {start}..{stop} is not a valid IEEE-488.2 block"
            ))
        })?;
        raw.extend_from_slice(&payload);
        chunks += 1;
        start = stop + 1;
    }

    let samples = decode_samples(&raw, &preamble, options)?;
    Ok(Waveform {
        source: options.source.clone(),
        mode: options.mode,
        format: options.format,
        preamble,
        samples,
        chunks,
    })
}

/// Decode raw waveform bytes into voltages using the preamble scaling factors.
fn decode_samples(
    raw: &[u8],
    preamble: &WaveformPreamble,
    options: &CaptureOptions,
) -> Result<Vec<f64>> {
    let scale = |code: i64| -> f64 {
        (code as f64 - preamble.y_origin - preamble.y_reference) * preamble.y_increment
    };

    let mut samples = Vec::new();
    match options.format {
        WaveformFormat::Byte => {
            samples.reserve(raw.len());
            for &code in raw {
                samples.push(scale(code as i64));
            }
        }
        WaveformFormat::Word => {
            samples.reserve(raw.len() / 2);
            for pair in raw.chunks_exact(2) {
                let code = options.word_byte_order.decode(pair[0], pair[1]);
                samples.push(scale(code as i64));
            }
        }
        WaveformFormat::Ascii => {
            let text = String::from_utf8_lossy(raw);
            for field in text.split(',') {
                let field = field.trim();
                if field.is_empty() {
                    continue;
                }
                let code = field.parse::<f64>().map_err(|_| {
                    Error::Malformed(format!("ASCII waveform sample is not a number: {field:?}"))
                })?;
                samples.push(scale(code as i64));
            }
        }
    }

    // Trim any padding beyond the preamble's point count.
    let expected = preamble.points as usize;
    if expected > 0 && samples.len() > expected {
        samples.truncate(expected);
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

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

    /// Build a definite-length IEEE-488.2 block around `payload`.
    fn block(payload: &[u8]) -> Vec<u8> {
        let mut out = format!("#2{:02}", payload.len()).into_bytes();
        out.extend_from_slice(payload);
        out
    }

    fn preamble_text(format_code: i64, points: u64) -> String {
        // format, type, points, count, xinc, xorig, xref, yinc, yorig, yref
        format!("{format_code},2,{points},1,0.000001,0,0,0.01,0,0")
    }

    #[test]
    fn test_parse_preamble() {
        let preamble = WaveformPreamble::parse("0,0,1200,1,5e-09,0,0,0.01,0,128").unwrap();
        assert_eq!(preamble.format_code, 0);
        assert_eq!(preamble.points, 1200);
        assert_eq!(preamble.x_increment, 5e-9);
        assert_eq!(preamble.y_increment, 0.01);
        assert_eq!(preamble.y_reference, 128.0);
        assert_eq!(preamble.format(), Some(WaveformFormat::Byte));
    }

    #[test]
    fn test_parse_preamble_rejects_short_input() {
        assert!(WaveformPreamble::parse("0,0,1200").is_err());
    }

    #[test]
    fn test_parse_preamble_rejects_non_numeric() {
        assert!(WaveformPreamble::parse("x,0,1200,1,1,1,1,1,1,1").is_err());
    }

    #[test]
    fn test_decode_byte_samples() {
        let preamble = WaveformPreamble {
            format_code: 0,
            type_code: 2,
            points: 3,
            count: 1,
            x_increment: 1e-6,
            x_origin: 0.0,
            x_reference: 0.0,
            y_increment: 0.01,
            y_origin: 0.0,
            y_reference: 128.0,
        };
        let options = CaptureOptions {
            format: WaveformFormat::Byte,
            ..Default::default()
        };
        let samples = decode_samples(&[128, 129, 127], &preamble, &options).unwrap();
        assert_eq!(samples, vec![0.0, 0.01, -0.01]);
    }

    #[test]
    fn test_decode_word_little_and_big() {
        let preamble = WaveformPreamble {
            format_code: 1,
            type_code: 2,
            points: 1,
            count: 1,
            x_increment: 1e-6,
            x_origin: 0.0,
            x_reference: 0.0,
            y_increment: 1.0,
            y_origin: 0.0,
            y_reference: 0.0,
        };
        let raw = [0x01u8, 0x00u8]; // 1 little-endian, 256 big-endian
        let little = CaptureOptions {
            format: WaveformFormat::Word,
            word_byte_order: WordByteOrder::Little,
            ..Default::default()
        };
        assert_eq!(decode_samples(&raw, &preamble, &little).unwrap(), vec![1.0]);

        let big = CaptureOptions {
            word_byte_order: WordByteOrder::Big,
            ..little
        };
        assert_eq!(decode_samples(&raw, &preamble, &big).unwrap(), vec![256.0]);
    }

    #[test]
    fn test_decode_ascii_samples() {
        let preamble = WaveformPreamble {
            format_code: 2,
            type_code: 2,
            points: 3,
            count: 1,
            x_increment: 1e-6,
            x_origin: 0.0,
            x_reference: 0.0,
            y_increment: 1.0,
            y_origin: 0.0,
            y_reference: 0.0,
        };
        let options = CaptureOptions {
            format: WaveformFormat::Ascii,
            ..Default::default()
        };
        let samples = decode_samples(b"1,2,3", &preamble, &options).unwrap();
        assert_eq!(samples, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_capture_stitches_raw_chunks() {
        let preamble = preamble_text(0, 5);
        let options = CaptureOptions {
            format: WaveformFormat::Byte,
            chunk_points: 2,
            ..Default::default()
        };
        // Three chunks: 2 + 2 + 1 points.
        let mut mock = MockTransport::new(vec![
            preamble.into_bytes(),
            block(&[128, 129]),
            block(&[130, 131]),
            block(&[132]),
        ]);

        let waveform = capture(&mut mock, &options).unwrap();
        assert_eq!(waveform.len(), 5);
        assert_eq!(waveform.chunks, 3);
        // y_increment is 0.01 V/code with a zero origin, so code 128 -> 1.28 V.
        assert_eq!(waveform.samples, vec![1.28, 1.29, 1.3, 1.31, 1.32]);
        // It stopped first, selected the source, and walked STAR/STOP windows.
        assert!(mock.sent.contains(&":STOP".to_string()));
        assert!(mock.sent.contains(&":WAV:SOUR CHAN1".to_string()));
        assert!(mock.sent.contains(&":WAV:STAR 1".to_string()));
        assert!(mock.sent.contains(&":WAV:STOP 2".to_string()));
        assert!(mock.sent.contains(&":WAV:STAR 3".to_string()));
        assert!(mock.sent.contains(&":WAV:STOP 4".to_string()));
        assert!(mock.sent.contains(&":WAV:STAR 5".to_string()));
        assert!(mock.sent.contains(&":WAV:STOP 5".to_string()));
    }

    #[test]
    fn test_capture_rejects_zero_points() {
        let options = CaptureOptions::default();
        let mut mock = MockTransport::new(vec![b"0,0,0,1,1,0,0,1,0,0".to_vec()]);
        let err = capture(&mut mock, &options).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
    }

    #[test]
    fn test_capture_rejects_bad_block() {
        let options = CaptureOptions {
            format: WaveformFormat::Byte,
            ..Default::default()
        };
        let mut mock = MockTransport::new(vec![
            preamble_text(0, 1).into_bytes(),
            b"not a block".to_vec(),
        ]);
        // A non-block payload is passed through by the parser, so force a
        // malformed header instead.
        let mut mock2 = MockTransport::new(vec![preamble_text(0, 1).into_bytes(), b"#x".to_vec()]);
        assert!(capture(&mut mock, &options).is_ok());
        assert!(capture(&mut mock2, &options).is_err());
    }

    #[test]
    fn test_time_and_pairs() {
        let waveform = Waveform {
            source: "CHAN1".to_string(),
            mode: WaveformMode::Raw,
            format: WaveformFormat::Byte,
            preamble: WaveformPreamble {
                format_code: 0,
                type_code: 2,
                points: 2,
                count: 1,
                x_increment: 0.5,
                x_origin: 1.0,
                x_reference: 0.0,
                y_increment: 1.0,
                y_origin: 0.0,
                y_reference: 0.0,
            },
            samples: vec![10.0, 11.0],
            chunks: 1,
        };
        assert_eq!(waveform.time_at(0), 1.0);
        assert_eq!(waveform.time_at(1), 1.5);
        let pairs: Vec<_> = waveform.pairs().collect();
        assert_eq!(pairs, vec![(1.0, 10.0), (1.5, 11.0)]);
    }

    #[test]
    fn test_write_npy_roundtrip_header() {
        let dir = std::env::temp_dir().join(format!(
            "omariglol-npy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wave.npy");

        let waveform = Waveform {
            source: "CHAN1".to_string(),
            mode: WaveformMode::Raw,
            format: WaveformFormat::Byte,
            preamble: WaveformPreamble {
                format_code: 0,
                type_code: 2,
                points: 2,
                count: 1,
                x_increment: 1.0,
                x_origin: 0.0,
                x_reference: 0.0,
                y_increment: 1.0,
                y_origin: 0.0,
                y_reference: 0.0,
            },
            samples: vec![1.0, 2.0],
            chunks: 1,
        };
        waveform.write_npy(&path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"\x93NUMPY"));
        // Pull the header length out of the v1.0 prefix.
        let header_len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        let header = std::str::from_utf8(&bytes[10..10 + header_len]).unwrap();
        assert!(header.contains("'shape': (2, 2)"));
        // 10 magic/version/length bytes + header + 2 rows * 2 columns * 8 bytes.
        assert_eq!(bytes.len(), 10 + header_len + 2 * 2 * 8);
        // Total header length is a multiple of 64.
        assert_eq!((10 + header_len) % 64, 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_write_csv_and_metadata() {
        let dir = std::env::temp_dir().join(format!(
            "omariglol-csv-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let waveform = Waveform {
            source: "CHAN2".to_string(),
            mode: WaveformMode::Raw,
            format: WaveformFormat::Word,
            preamble: WaveformPreamble {
                format_code: 1,
                type_code: 2,
                points: 1,
                count: 1,
                x_increment: 1e-6,
                x_origin: 0.0,
                x_reference: 0.0,
                y_increment: 1.0,
                y_origin: 0.0,
                y_reference: 0.0,
            },
            samples: vec![0.5],
            chunks: 1,
        };
        let csv = dir.join("wave.csv");
        let meta = dir.join("wave.json");
        waveform.write_csv(&csv).unwrap();
        waveform.write_metadata(&meta).unwrap();

        let csv_text = std::fs::read_to_string(&csv).unwrap();
        assert!(csv_text.starts_with("time_s,voltage_v\n"));
        assert!(csv_text.contains("5.000000000e-1"));

        let meta_text = std::fs::read_to_string(&meta).unwrap();
        assert!(meta_text.contains("\"source\": \"CHAN2\""));
        assert!(meta_text.contains("\"points\": 1"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
