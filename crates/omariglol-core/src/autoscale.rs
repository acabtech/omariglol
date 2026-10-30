//! Fan-out autoscale across a workspace's instruments.
//!
//! The Omarchy panel's AutoSet button asks every instrument that supports auto
//! scaling to fit its display to the incoming signal. The fan-out lives here so
//! the CLI and any other frontend share one implementation, matching the
//! measurement fan-out in [`crate::measure`].
//!
//! Instruments without an `autoscale` operation — everything that is not an
//! oscilloscope — are reported as skipped rather than failed.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::device::Target;
use crate::driver::{InstrumentDriver, InstrumentType, classify_idn};
use crate::instrument::Instrument;
use crate::transport::Session;
use crate::workspace::Workspace;

/// One instrument's autoscale outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleResult {
    /// The workspace instrument id.
    pub instrument_id: String,
    /// The instrument's display label.
    pub label: String,
    /// The instrument's address.
    pub address: String,
    /// The type inferred from `*IDN?`.
    pub instrument_type: InstrumentType,
    /// True when the autoscale command was sent.
    pub executed: bool,
    /// The failure, when the instrument could not be identified or commanded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock time for this instrument, in milliseconds.
    pub elapsed_ms: f64,
}

impl AutoscaleResult {
    /// True when the instrument answered and supports autoscale.
    pub fn is_executed(&self) -> bool {
        self.executed && self.error.is_none()
    }

    /// True when the instrument does not expose an autoscale operation.
    pub fn is_skipped(&self) -> bool {
        !self.executed && self.error.is_none()
    }
}

/// The whole fan-out: one result per instrument in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleReport {
    /// The workspace that was autoscaled.
    pub workspace: String,
    /// One result per instrument, in workspace order.
    pub results: Vec<AutoscaleResult>,
    /// Wall-clock time for the whole run, in milliseconds.
    pub elapsed_ms: f64,
}

impl AutoscaleReport {
    /// The number of instruments that ran autoscale.
    pub fn executed(&self) -> usize {
        self.results.iter().filter(|r| r.is_executed()).count()
    }

    /// The number of instruments skipped because they do not support it.
    pub fn skipped(&self) -> usize {
        self.results.iter().filter(|r| r.is_skipped()).count()
    }

    /// The number of instruments that failed.
    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| r.error.is_some()).count()
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

/// Connect to one instrument, identify it, and send its autoscale command.
///
/// Supporting the operation is read from the driver's command table, so a
/// non-scope is a clean skip rather than an error.
fn autoscale_one(instrument: &Instrument) -> AutoscaleResult {
    let started = Instant::now();
    let elapsed = |started: Instant| started.elapsed().as_secs_f64() * 1000.0;

    let mut result = AutoscaleResult {
        instrument_id: instrument.id.to_string(),
        label: instrument.label(),
        address: instrument.device.address.to_string(),
        instrument_type: instrument
            .instrument_type()
            .unwrap_or(InstrumentType::Unknown),
        executed: false,
        error: None,
        elapsed_ms: 0.0,
    };

    let target = target_for(instrument);
    let mut session = match Session::open(target) {
        Ok(session) => session,
        Err(err) => {
            result.error = Some(err.to_string());
            result.elapsed_ms = elapsed(started);
            return result;
        }
    };

    let idn = match session.identify() {
        Ok(idn) => idn,
        Err(err) => {
            result.error = Some(err.to_string());
            result.elapsed_ms = elapsed(started);
            return result;
        }
    };

    result.instrument_type = classify_idn(&idn);
    let driver = InstrumentDriver::from_idn(&idn);
    let Some(command) = driver.command("autoscale") else {
        result.elapsed_ms = elapsed(started);
        return result;
    };

    match session.send(&command.scpi) {
        Ok(()) => result.executed = true,
        Err(err) => result.error = Some(err.to_string()),
    }

    result.elapsed_ms = elapsed(started);
    result
}

/// Autoscale every instrument in a workspace that supports it, in parallel.
///
/// One thread per instrument; a panicking thread becomes a per-instrument error
/// rather than aborting the run.
pub fn autoscale_workspace(workspace: &Workspace) -> AutoscaleReport {
    let started = Instant::now();

    let results: Vec<AutoscaleResult> = std::thread::scope(|scope| {
        let handles: Vec<(&Instrument, _)> = workspace
            .instruments
            .iter()
            .map(|instrument| (instrument, scope.spawn(move || autoscale_one(instrument))))
            .collect();

        handles
            .into_iter()
            .map(|(instrument, handle)| {
                handle.join().unwrap_or_else(|_| AutoscaleResult {
                    instrument_id: instrument.id.to_string(),
                    label: instrument.label(),
                    address: instrument.device.address.to_string(),
                    instrument_type: instrument
                        .instrument_type()
                        .unwrap_or(InstrumentType::Unknown),
                    executed: false,
                    error: Some("autoscale thread panicked".to_string()),
                    elapsed_ms: 0.0,
                })
            })
            .collect()
    });

    AutoscaleReport {
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
    fn test_empty_workspace() {
        let workspace = Workspace::new("Bench");
        let report = autoscale_workspace(&workspace);
        assert_eq!(report.workspace, "Bench");
        assert!(report.results.is_empty());
        assert_eq!(report.executed(), 0);
        assert_eq!(report.skipped(), 0);
        assert_eq!(report.failed(), 0);
    }

    #[test]
    fn test_report_aggregation() {
        let report = AutoscaleReport {
            workspace: "Bench".to_string(),
            elapsed_ms: 1.0,
            results: vec![
                AutoscaleResult {
                    instrument_id: "a".to_string(),
                    label: "a".to_string(),
                    address: "10.0.0.1".to_string(),
                    instrument_type: InstrumentType::Oscilloscope,
                    executed: true,
                    error: None,
                    elapsed_ms: 1.0,
                },
                AutoscaleResult {
                    instrument_id: "b".to_string(),
                    label: "b".to_string(),
                    address: "10.0.0.2".to_string(),
                    instrument_type: InstrumentType::PowerSupply,
                    executed: false,
                    error: None,
                    elapsed_ms: 1.0,
                },
                AutoscaleResult {
                    instrument_id: "c".to_string(),
                    label: "c".to_string(),
                    address: "10.0.0.3".to_string(),
                    instrument_type: InstrumentType::Unknown,
                    executed: false,
                    error: Some("timed out".to_string()),
                    elapsed_ms: 1.0,
                },
            ],
        };
        assert_eq!(report.executed(), 1);
        assert_eq!(report.skipped(), 1);
        assert_eq!(report.failed(), 1);
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
}
