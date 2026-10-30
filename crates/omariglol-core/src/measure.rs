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
use crate::driver::{InstrumentType, classify_idn, measure_all_commands};
use crate::instrument::Instrument;
use crate::transport::Session;
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
            address: instrument.device.address.to_string(),
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
    let mut target = Target::new(instrument.device.address, instrument.config.transport)
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

    // Best-effort: ask the instrument to show all measurements before the
    // snapshot is taken, so the screenshot is more useful. An instrument
    // without such a mode rejects the write; that is not a measurement failure.
    for setup in measure_all_commands(&idn) {
        let _ = session.send(setup);
    }

    let default = default_measurement(instrument_type);
    let command = command_override
        .map(str::to_string)
        .or_else(|| default.map(str::to_string))
        .unwrap_or_else(|| "*IDN?".to_string());
    let is_measurement = command_override.is_some() || default.is_some();

    let mut result = MeasurementResult {
        instrument_id: instrument.id.to_string(),
        label: instrument.label(),
        address: instrument.device.address.to_string(),
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
            Ok(value) => result.value = Some(value),
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
