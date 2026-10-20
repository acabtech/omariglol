//! SCPI request/response benchmarking, mirroring `lxi benchmark`.
//!
//! Measures how fast an instrument answers a query — the practical number that
//! decides whether a measurement loop can run at 10 Hz or 200 Hz.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::transport::ScpiTransport;

/// Statistics from one benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    /// The query that was repeated.
    pub command: String,
    /// Number of successful round trips.
    pub iterations: usize,
    /// Failures encountered (the run continues past them).
    pub failures: usize,
    /// Total wall-clock time for the run.
    pub elapsed_ms: f64,
    /// Fastest round trip.
    pub min_ms: f64,
    /// Slowest round trip.
    pub max_ms: f64,
    /// Arithmetic mean round trip.
    pub mean_ms: f64,
    /// Median round trip.
    pub median_ms: f64,
    /// 95th percentile round trip.
    pub p95_ms: f64,
    /// Standard deviation of the round trips.
    pub stddev_ms: f64,
    /// Round trips per second.
    pub messages_per_second: f64,
    /// Total response bytes received.
    pub bytes_received: usize,
}

impl BenchmarkReport {
    /// Throughput in kilobytes per second of response payload.
    pub fn kbytes_per_second(&self) -> f64 {
        let _ = self;
        todo!("derive throughput from elapsed time and bytes received")
    }
}

/// How to run a benchmark.
#[derive(Debug, Clone)]
pub struct BenchmarkOptions {
    /// The query to repeat. `*IDN?` matches lxi-tools' default.
    pub command: String,
    /// Number of round trips to attempt.
    pub iterations: usize,
    /// Optional ceiling on total runtime.
    pub max_duration: Option<Duration>,
    /// Stop early after this many consecutive failures.
    pub max_consecutive_failures: usize,
}

impl Default for BenchmarkOptions {
    fn default() -> Self {
        BenchmarkOptions {
            command: "*IDN?".to_string(),
            iterations: 100,
            max_duration: None,
            max_consecutive_failures: 5,
        }
    }
}

/// Run a benchmark against an open transport.
///
/// `on_progress` is called after each round trip with the iteration count and
/// the sample in milliseconds, which is what lets the GUI draw a live trace.
pub fn run(
    transport: &mut dyn ScpiTransport,
    options: &BenchmarkOptions,
    on_progress: impl FnMut(usize, f64),
) -> Result<BenchmarkReport> {
    let _ = (transport, options, on_progress);
    todo!("time repeated queries and summarise the samples")
}

/// Build a report from raw samples.
pub(crate) fn summarise(
    command: String,
    samples: Vec<f64>,
    failures: usize,
    elapsed: Duration,
    bytes_received: usize,
) -> BenchmarkReport {
    let _ = (command, samples, failures, elapsed, bytes_received);
    todo!("compute min/max/mean/median/p95/stddev and throughput")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_kbytes_per_second() {
        todo!("derive throughput from elapsed time and bytes received")
    }

    #[test]
    #[ignore]
    fn test_summarise() {
        todo!("compute min/max/mean/median/p95/stddev and throughput")
    }

    #[test]
    #[ignore]
    fn test_benchmark_default_options() {
        let opts = BenchmarkOptions::default();
        assert_eq!(opts.command, "*IDN?");
        assert_eq!(opts.iterations, 100);
        assert_eq!(opts.max_consecutive_failures, 5);
    }

    #[test]
    #[ignore]
    fn test_benchmark_continues_past_failures() {
        todo!("run continues past individual failures and records them")
    }

    #[test]
    #[ignore]
    fn test_benchmark_progress_callback() {
        todo!("progress callbacks fire once per round trip")
    }

    #[test]
    #[ignore]
    fn test_benchmark_throughput_consistency() {
        todo!("messages_per_second and kbytes_per_second are consistent with elapsed time")
    }
}
