//! Fan-out measurement across a workspace's instruments.
//!
//! The Omarchy shell plugin's trigger measures every instrument in the active
//! workspace at once. That fan-out lives here, in the core, so the CLI, the
//! desktop app and the plugin all share one implementation (`AC-1`, `AC-5`).
//!
//! The core stays blocking (`AC-2`): [`measure_workspace`] uses
//! [`std::thread::scope`] to run one instrument per thread, which is the
//! caller's choice of concurrency rather than an async runtime.
//!
//! See `plans/omarchy-integration.md` §3 for the command surface and the JSON
//! shape this produces.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::device::Target;
use crate::driver::{InstrumentDriver, InstrumentType, classify_idn};
use crate::instrument::Instrument;
use crate::transport::{ScpiTransport, Session};
use crate::workspace::Workspace;

/// The default measurement query for an instrument type.
///
/// A scope, a DMM and a spectrum analyzer each have a natural scalar read-out;
/// everything else (power supplies, generators, loads, switches) has no single
/// "measurement", so it is probed for liveness only. `--command` overrides this
/// for every instrument.
pub fn default_measurement(instrument_type: InstrumentType) -> Option<&'static str> {
    match instrument_type {
        InstrumentType::Oscilloscope => Some(":MEAS:FREQ?"),
        InstrumentType::Multimeter => Some(":MEAS:VOLT?"),
        InstrumentType::SpectrumAnalyzer => Some(":CALC:MARK:Y?"),
        InstrumentType::PowerSupply
        | InstrumentType::SignalGenerator
        | InstrumentType::ElectronicLoad
        | InstrumentType::SwitchModule
        | InstrumentType::Unknown => None,
    }
}

/// A measurement read from an instrument while taking a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedMeasurement {
    /// The driver operation name, e.g. `measure_vpp`.
    pub name: String,
    /// The SCPI mnemonic shown as the CSV column, e.g. `VPP`.
    pub abbreviation: String,
    /// The instrument's response.
    pub value: String,
}

/// The measurement queries a driver exposes, as `(name, abbreviation, scpi)`.
///
/// Only queries in the driver's `measure` group are considered, so acquisition
/// and display commands are never mistaken for readings.
pub fn measurement_queries(idn: &str) -> Vec<(String, String, String)> {
    InstrumentDriver::from_idn(idn)
        .commands
        .into_iter()
        .filter(|command| command.group == "measure" && command.query)
        .map(|command| {
            let abbreviation = command
                .scpi
                .trim()
                .trim_end_matches('?')
                .rsplit(':')
                .next()
                .unwrap_or(command.scpi.as_str())
                .to_string();
            (command.name, abbreviation, command.scpi)
        })
        .collect()
}

/// True when a response is a usable reading rather than a placeholder.
///
/// Rigol instruments answer an unavailable measurement with `9.9E37`; that
/// sentinel, empty replies and non-numeric text are all skipped.
fn is_usable_measurement(value: &str) -> bool {
    match value.trim().parse::<f64>() {
        Ok(number) => number.is_finite() && number.abs() < 9.0e37,
        Err(_) => false,
    }
}

/// Describe why a reply was not a usable reading.
///
/// Reports the instrument's original reply, not its expanded decimal form: the
/// Rigol `9.9E37` sentinel would otherwise appear as a 38-digit integer.
fn unusable_reading_error(reply: &str) -> String {
    let reply = reply.trim();
    if reply.is_empty() {
        "no reading (empty reply)".to_string()
    } else {
        format!("no reading (instrument returned {reply})")
    }
}

/// Render a reading as a plain decimal number.
///
/// Instruments answer with scientific notation (`3.120000e+00`,
/// `-2.081299e-02`). Keeping that text verbatim leaks exponents into the CSV,
/// the JSON report and the shell panel, so parse it and let `f64`'s `Display`
/// write the shortest decimal form instead: `3.12`, `-0.02081299`, `1000`.
/// Non-numeric replies (`*IDN?`, error text) pass through unchanged.
pub fn plain_number(value: &str) -> String {
    let trimmed = value.trim();
    match trimmed.parse::<f64>() {
        Ok(number) if number.is_finite() => format!("{number}"),
        _ => trimmed.to_string(),
    }
}

/// Query every measurement an instrument's driver exposes.
///
/// Errors and placeholder answers are omitted, so the result is only what the
/// instrument actually detected. The caller is responsible for having already
/// identified the instrument (`idn`).
pub fn collect_measurements(
    transport: &mut dyn ScpiTransport,
    idn: &str,
) -> Vec<DetectedMeasurement> {
    let mut detected = Vec::new();
    for (name, abbreviation, scpi) in measurement_queries(idn) {
        let Ok(raw) = transport.query(&scpi) else {
            continue;
        };
        let value = plain_number(&String::from_utf8_lossy(&raw));
        if is_usable_measurement(&value) {
            detected.push(DetectedMeasurement {
                name,
                abbreviation,
                value,
            });
        }
    }
    detected
}

/// One instrument's measurement outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementResult {
    /// The workspace instrument id.
    pub instrument_id: String,
    /// The instrument's display label.
    pub label: String,
    /// The instrument's address.
    pub address: String,
    /// The type inferred from `*IDN?`.
    pub instrument_type: InstrumentType,
    /// The query that was run.
    pub command: String,
    /// The response, when the query succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The failure, when the query did not succeed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock time for this instrument, in milliseconds.
    pub elapsed_ms: f64,
    /// True when the command was a real measurement rather than a liveness probe.
    pub is_measurement: bool,
}

impl MeasurementResult {
    /// A result for an instrument that could not be reached at all.
    fn failed(instrument: &Instrument, error: impl Into<String>, elapsed_ms: f64) -> Self {
        MeasurementResult {
            instrument_id: instrument.id.to_string(),
            label: instrument.label(),
            address: instrument.device.display_address(),
            instrument_type: instrument
                .instrument_type()
                .unwrap_or(InstrumentType::Unknown),
            command: String::new(),
            value: None,
            error: Some(error.into()),
            elapsed_ms,
            is_measurement: false,
        }
    }

    /// True when the instrument answered.
    pub fn is_ok(&self) -> bool {
        self.error.is_none()
    }
}

/// The whole fan-out: one result per instrument in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasureReport {
    /// The workspace that was measured.
    pub workspace: String,
    /// One result per instrument, in workspace order.
    pub results: Vec<MeasurementResult>,
    /// Wall-clock time for the whole run, in milliseconds.
    pub elapsed_ms: f64,
}

impl MeasureReport {
    /// The number of instruments that answered.
    pub fn succeeded(&self) -> usize {
        self.results.iter().filter(|r| r.is_ok()).count()
    }

    /// The number of instruments that failed.
    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| !r.is_ok()).count()
    }

    /// The number of instruments that produced a real measurement.
    pub fn measured(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.is_measurement && r.is_ok())
            .count()
    }
}

/// Build a transport target from an instrument's configuration.
fn target_for(instrument: &Instrument) -> Target {
    let mut target = Target::from_device(&instrument.device, instrument.config.transport)
        .with_timeout(instrument.config.timeout());
    if let Some(port) = instrument.config.port {
        target = target.with_port(port);
    }
    target.device_name = instrument.device.device_name.clone();
    target
}

/// Connect to one instrument, identify it, and run its measurement.
///
/// The identity query always runs first, so the default measurement is chosen
/// from the instrument's real `*IDN?` rather than from whatever the workspace
/// cached. A failure at any step is recorded on the result, never returned as an
/// error, so one dead instrument cannot abort the run.
fn measure_one(instrument: &Instrument, command_override: Option<&str>) -> MeasurementResult {
    let started = Instant::now();
    let elapsed = |started: Instant| started.elapsed().as_secs_f64() * 1000.0;

    let target = target_for(instrument);
    let mut session = match Session::open(target) {
        Ok(session) => session,
        Err(err) => {
            return MeasurementResult::failed(instrument, err.to_string(), elapsed(started));
        }
    };

    let idn = match session.identify() {
        Ok(idn) => idn,
        Err(err) => {
            return MeasurementResult::failed(instrument, err.to_string(), elapsed(started));
        }
    };
    let instrument_type = classify_idn(&idn);

    let default = default_measurement(instrument_type);
    let command = command_override
        .map(str::to_string)
        .or_else(|| default.map(str::to_string))
        .unwrap_or_else(|| "*IDN?".to_string());
    let is_measurement = command_override.is_some() || default.is_some();

    let mut result = MeasurementResult {
        instrument_id: instrument.id.to_string(),
        label: instrument.label(),
        address: instrument.device.display_address(),
        instrument_type,
        command: command.clone(),
        value: None,
        error: None,
        elapsed_ms: 0.0,
        is_measurement,
    };

    // `*IDN?` was already answered above; reuse it rather than querying twice.
    if command == "*IDN?" {
        result.value = Some(idn);
    } else {
        match session.query(&command) {
            Ok(reply) => {
                let value = plain_number(&reply);
                // Instruments answer an unavailable measurement with a
                // sentinel (`9.9E37` on Rigol scopes). Do not surface that as a
                // real reading, nor expand it into a 38-digit integer.
                if is_usable_measurement(&value) {
                    result.value = Some(value);
                } else {
                    result.error = Some(unusable_reading_error(&reply));
                }
            }
            Err(err) => result.error = Some(err.to_string()),
        }
    }

    result.elapsed_ms = elapsed(started);
    result
}

/// Measure every instrument in a workspace, in parallel.
///
/// One thread per instrument; a panicking thread becomes a per-instrument error
/// rather than aborting the run.
pub fn measure_workspace(workspace: &Workspace, command_override: Option<&str>) -> MeasureReport {
    let started = Instant::now();

    let results: Vec<MeasurementResult> = std::thread::scope(|scope| {
        let handles: Vec<(&Instrument, _)> = workspace
            .instruments
            .iter()
            .map(|instrument| {
                (
                    instrument,
                    scope.spawn(move || measure_one(instrument, command_override)),
                )
            })
            .collect();

        handles
            .into_iter()
            .map(|(instrument, handle)| {
                handle.join().unwrap_or_else(|_| {
                    MeasurementResult::failed(instrument, "measurement thread panicked", 0.0)
                })
            })
            .collect()
    });

    MeasureReport {
        workspace: workspace.name.clone(),
        results,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Device;
    use std::net::IpAddr;

    fn device(addr: &str) -> Device {
        Device::manual(addr.parse::<IpAddr>().unwrap())
    }

    #[test]
    fn test_default_measurement_table() {
        assert_eq!(
            default_measurement(InstrumentType::Oscilloscope),
            Some(":MEAS:FREQ?")
        );
        assert_eq!(
            default_measurement(InstrumentType::Multimeter),
            Some(":MEAS:VOLT?")
        );
        assert_eq!(
            default_measurement(InstrumentType::SpectrumAnalyzer),
            Some(":CALC:MARK:Y?")
        );
        // Non-measurement instruments have no default.
        assert_eq!(default_measurement(InstrumentType::PowerSupply), None);
        assert_eq!(default_measurement(InstrumentType::SignalGenerator), None);
        assert_eq!(default_measurement(InstrumentType::Unknown), None);
    }

    #[test]
    fn test_measurement_queries_use_scpi_mnemonics() {
        let queries = measurement_queries("RIGOL TECHNOLOGIES,DS1054Z,SN,1.0");
        assert!(
            queries
                .iter()
                .any(|(_, abbreviation, scpi)| abbreviation == "VPP" && scpi == ":MEAS:VPP?")
        );
        // Writes and non-measurement commands are excluded.
        assert!(!queries.iter().any(|(name, ..)| name == "measure_clear"));
        assert!(!queries.iter().any(|(name, ..)| name == "run"));
    }

    #[test]
    fn test_plain_number_drops_scientific_notation() {
        assert_eq!(plain_number("3.120000e+00"), "3.12");
        assert_eq!(plain_number("-2.081299e-02"), "-0.02081299");
        assert_eq!(plain_number("5.000000e-04"), "0.0005");
        assert_eq!(plain_number("9.999999e+02"), "999.9999");
        // Whole numbers lose the trailing `.0` and stay integers.
        assert_eq!(plain_number("1.000000e+03"), "1000");
        assert_eq!(plain_number("3.000000e+00"), "3");
        // Non-numeric replies are preserved.
        assert_eq!(
            plain_number("RIGOL TECHNOLOGIES,DS1054Z,0,1.0"),
            "RIGOL TECHNOLOGIES,DS1054Z,0,1.0"
        );
        assert_eq!(plain_number(""), "");
    }

    #[test]
    fn test_is_usable_measurement_rejects_placeholders() {
        assert!(is_usable_measurement("1.000E+03"));
        assert!(is_usable_measurement("-0.5"));
        // Rigol's "no measurement" sentinel.
        assert!(!is_usable_measurement("9.9E37"));
        assert!(!is_usable_measurement(""));
        assert!(!is_usable_measurement("****"));
    }

    #[test]
    fn test_unusable_reading_error_keeps_raw_reply() {
        let message = unusable_reading_error("9.9E37");
        assert!(message.contains("9.9E37"));
        assert!(!message.contains("99000000000000000000000000000000000000"));
        assert_eq!(unusable_reading_error("  "), "no reading (empty reply)");
    }

    #[test]
    fn test_target_for_uses_config() {
        let mut instrument = Instrument::from_device(device("192.168.1.50"));
        instrument.config.transport = crate::device::Transport::Raw;
        instrument.config.port = Some(5555);
        instrument.config.timeout_secs = 9;
        let target = target_for(&instrument);
        assert_eq!(target.transport, crate::device::Transport::Raw);
        assert_eq!(target.port, Some(5555));
        assert_eq!(target.timeout, std::time::Duration::from_secs(9));
        assert_eq!(target.device_name, "inst0");
    }

    #[test]
    fn test_report_aggregation() {
        let report = MeasureReport {
            workspace: "Bench".to_string(),
            elapsed_ms: 1.0,
            results: vec![
                MeasurementResult {
                    instrument_id: "a".to_string(),
                    label: "a".to_string(),
                    address: "10.0.0.1".to_string(),
                    instrument_type: InstrumentType::Oscilloscope,
                    command: ":MEAS:FREQ?".to_string(),
                    value: Some("1".to_string()),
                    error: None,
                    elapsed_ms: 1.0,
                    is_measurement: true,
                },
                MeasurementResult {
                    instrument_id: "b".to_string(),
                    label: "b".to_string(),
                    address: "10.0.0.2".to_string(),
                    instrument_type: InstrumentType::PowerSupply,
                    command: "*IDN?".to_string(),
                    value: Some("PSU".to_string()),
                    error: None,
                    elapsed_ms: 1.0,
                    is_measurement: false,
                },
                MeasurementResult {
                    instrument_id: "c".to_string(),
                    label: "c".to_string(),
                    address: "10.0.0.3".to_string(),
                    instrument_type: InstrumentType::Unknown,
                    command: String::new(),
                    value: None,
                    error: Some("timed out".to_string()),
                    elapsed_ms: 1.0,
                    is_measurement: false,
                },
            ],
        };
        assert_eq!(report.succeeded(), 2);
        assert_eq!(report.failed(), 1);
        assert_eq!(report.measured(), 1);
    }

    #[test]
    fn test_measure_workspace_empty() {
        let workspace = Workspace::new("Empty");
        let report = measure_workspace(&workspace, None);
        assert_eq!(report.workspace, "Empty");
        assert!(report.results.is_empty());
        assert_eq!(report.succeeded(), 0);
    }

    #[test]
    fn test_measure_workspace_unreachable_is_per_instrument_error() {
        // A workspace with one instrument on a closed port: the run must
        // complete with a per-instrument error, not panic or hang.
        let mut workspace = Workspace::new("Bench");
        let id = workspace.add_instrument(device("127.0.0.1"));
        if let Some(instrument) = workspace.instrument_mut(&id) {
            instrument.config.transport = crate::device::Transport::Raw;
            instrument.config.port = Some(1); // nothing listens here
            instrument.config.timeout_secs = 1;
        }
        let report = measure_workspace(&workspace, None);
        assert_eq!(report.results.len(), 1);
        assert!(!report.results[0].is_ok());
        assert_eq!(report.failed(), 1);
    }
}
