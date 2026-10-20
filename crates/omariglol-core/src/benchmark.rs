//! SCPI request/response benchmarking, mirroring `lxi benchmark`.
//!
//! Measures how fast an instrument answers a query — the practical number that
//! decides whether a measurement loop can run at 10 Hz or 200 Hz.

use std::time::{Duration, Instant};

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
        let seconds = self.elapsed_ms / 1000.0;
        if seconds <= 0.0 {
            return 0.0;
        }
        self.bytes_received as f64 / 1024.0 / seconds
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
            iterations: 25,
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
    mut on_progress: impl FnMut(usize, f64),
) -> Result<BenchmarkReport> {
    let start = Instant::now();
    let mut samples: Vec<f64> = Vec::with_capacity(options.iterations);
    let mut failures = 0usize;
    let mut consecutive_failures = 0usize;
    let mut bytes_received = 0usize;

    for iteration in 0..options.iterations {
        if let Some(max) = options.max_duration {
            if start.elapsed() >= max {
                break;
            }
        }

        let round_start = Instant::now();
        match transport.query(&options.command) {
            Ok(response) => {
                let sample_ms = round_start.elapsed().as_secs_f64() * 1000.0;
                bytes_received += response.len();
                samples.push(sample_ms);
                consecutive_failures = 0;
                on_progress(iteration + 1, sample_ms);
            }
            Err(_) => {
                failures += 1;
                consecutive_failures += 1;
                if options.max_consecutive_failures > 0
                    && consecutive_failures >= options.max_consecutive_failures
                {
                    break;
                }
            }
        }
    }

    Ok(summarise(
        options.command.clone(),
        samples,
        failures,
        start.elapsed(),
        bytes_received,
    ))
}

/// Build a report from raw samples.
pub(crate) fn summarise(
    command: String,
    mut samples: Vec<f64>,
    failures: usize,
    elapsed: Duration,
    bytes_received: usize,
) -> BenchmarkReport {
    let iterations = samples.len();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

    let (min_ms, max_ms, mean_ms, median_ms, p95_ms, stddev_ms) = if samples.is_empty() {
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    } else {
        let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
        let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mean = samples.iter().sum::<f64>() / iterations as f64;

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = median_of_sorted(&samples);
        let p95 = percentile_nearest_rank(&samples, 0.95);

        let variance = samples
            .iter()
            .map(|s| {
                let d = s - mean;
                d * d
            })
            .sum::<f64>()
            / iterations as f64;
        let stddev = variance.sqrt();

        (min, max, mean, median, p95, stddev)
    };

    let seconds = elapsed.as_secs_f64();
    let messages_per_second = if seconds > 0.0 {
        iterations as f64 / seconds
    } else {
        0.0
    };

    BenchmarkReport {
        command,
        iterations,
        failures,
        elapsed_ms,
        min_ms,
        max_ms,
        mean_ms,
        median_ms,
        p95_ms,
        stddev_ms,
        messages_per_second,
        bytes_received,
    }
}

/// Median of an already-sorted, non-empty slice.
fn median_of_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Nearest-rank percentile of an already-sorted, non-empty slice.
fn percentile_nearest_rank(sorted: &[f64], fraction: f64) -> f64 {
    let n = sorted.len();
    let rank = (fraction * n as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(n - 1);
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::collections::VecDeque;

    #[test]
    fn test_kbytes_per_second() {
        let report = BenchmarkReport {
            command: "*IDN?".to_string(),
            iterations: 10,
            failures: 0,
            elapsed_ms: 1000.0,
            min_ms: 0.0,
            max_ms: 0.0,
            mean_ms: 0.0,
            median_ms: 0.0,
            p95_ms: 0.0,
            stddev_ms: 0.0,
            messages_per_second: 10.0,
            bytes_received: 2048,
        };
        assert!((report.kbytes_per_second() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_kbytes_per_second_zero_elapsed() {
        let report = BenchmarkReport {
            command: "*IDN?".to_string(),
            iterations: 0,
            failures: 0,
            elapsed_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            mean_ms: 0.0,
            median_ms: 0.0,
            p95_ms: 0.0,
            stddev_ms: 0.0,
            messages_per_second: 0.0,
            bytes_received: 0,
        };
        assert_eq!(report.kbytes_per_second(), 0.0);
    }

    #[test]
    fn test_summarise() {
        // Known sample set: 1..=10 ms.
        let samples: Vec<f64> = (1..=10).map(|n| n as f64).collect();
        let report = summarise(
            "*IDN?".to_string(),
            samples,
            0,
            Duration::from_millis(1000),
            100,
        );
        assert_eq!(report.iterations, 10);
        assert_eq!(report.min_ms, 1.0);
        assert_eq!(report.max_ms, 10.0);
        assert!((report.mean_ms - 5.5).abs() < 1e-9);
        assert!((report.median_ms - 5.5).abs() < 1e-9);
        // Nearest-rank p95 of 10 samples is the 10th (index 9) = 10.
        assert_eq!(report.p95_ms, 10.0);
        // Population stddev of 1..=10 is sqrt(8.25).
        assert!((report.stddev_ms - 8.25f64.sqrt()).abs() < 1e-9);
        assert!((report.messages_per_second - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_summarise_odd_median() {
        let report = summarise(
            "q".to_string(),
            vec![3.0, 1.0, 2.0],
            0,
            Duration::from_millis(100),
            0,
        );
        assert_eq!(report.median_ms, 2.0);
        assert_eq!(report.min_ms, 1.0);
        assert_eq!(report.max_ms, 3.0);
    }

    #[test]
    fn test_summarise_empty() {
        let report = summarise("q".to_string(), vec![], 3, Duration::from_millis(50), 0);
        assert_eq!(report.iterations, 0);
        assert_eq!(report.failures, 3);
        assert_eq!(report.min_ms, 0.0);
        assert_eq!(report.mean_ms, 0.0);
        assert_eq!(report.median_ms, 0.0);
        assert_eq!(report.p95_ms, 0.0);
        assert_eq!(report.stddev_ms, 0.0);
    }

    #[test]
    fn test_benchmark_default_options() {
        let opts = BenchmarkOptions::default();
        assert_eq!(opts.command, "*IDN?");
        assert_eq!(opts.iterations, 25);
        assert_eq!(opts.max_consecutive_failures, 5);
        assert!(opts.max_duration.is_none());
    }

    /// A transport that replays a scripted sequence of results.
    struct MockTransport {
        results: VecDeque<std::result::Result<Vec<u8>, ()>>,
    }

    impl MockTransport {
        fn new(results: Vec<std::result::Result<Vec<u8>, ()>>) -> Self {
            MockTransport {
                results: results.into_iter().collect(),
            }
        }
    }

    impl ScpiTransport for MockTransport {
        fn send(&mut self, _command: &str) -> Result<()> {
            Ok(())
        }

        fn receive(&mut self) -> Result<Vec<u8>> {
            match self.results.pop_front() {
                Some(Ok(bytes)) => Ok(bytes),
                Some(Err(())) => Err(Error::Other("scripted failure".to_string())),
                None => Err(Error::Other("no result queued".to_string())),
            }
        }

        fn close(&mut self) -> Result<()> {
            Ok(())
        }

        fn description(&self) -> String {
            "mock".to_string()
        }
    }

    #[test]
    fn test_benchmark_continues_past_failures() {
        let mut transport = MockTransport::new(vec![
            Ok(b"a".to_vec()),
            Err(()),
            Ok(b"bb".to_vec()),
            Err(()),
            Ok(b"ccc".to_vec()),
        ]);
        let options = BenchmarkOptions {
            iterations: 5,
            max_consecutive_failures: 5,
            ..Default::default()
        };
        let report = run(&mut transport, &options, |_, _| {}).unwrap();
        assert_eq!(report.iterations, 3);
        assert_eq!(report.failures, 2);
        assert_eq!(report.bytes_received, 6);
    }

    #[test]
    fn test_benchmark_stops_after_consecutive_failures() {
        let mut transport =
            MockTransport::new(vec![Err(()), Err(()), Err(()), Ok(b"never".to_vec())]);
        let options = BenchmarkOptions {
            iterations: 10,
            max_consecutive_failures: 3,
            ..Default::default()
        };
        let report = run(&mut transport, &options, |_, _| {}).unwrap();
        assert_eq!(report.iterations, 0);
        assert_eq!(report.failures, 3);
    }

    #[test]
    fn test_benchmark_progress_callback() {
        let mut transport = MockTransport::new(vec![
            Ok(b"a".to_vec()),
            Ok(b"b".to_vec()),
            Ok(b"c".to_vec()),
        ]);
        let options = BenchmarkOptions {
            iterations: 3,
            ..Default::default()
        };
        let mut calls: Vec<usize> = Vec::new();
        let report = run(&mut transport, &options, |iteration, _ms| {
            calls.push(iteration);
        })
        .unwrap();
        assert_eq!(calls, vec![1, 2, 3]);
        assert_eq!(report.iterations, 3);
    }

    #[test]
    fn test_benchmark_throughput_consistency() {
        let mut transport = MockTransport::new(vec![Ok(vec![0u8; 1024]); 4]);
        let options = BenchmarkOptions {
            iterations: 4,
            ..Default::default()
        };
        let report = run(&mut transport, &options, |_, _| {}).unwrap();
        assert_eq!(report.bytes_received, 4096);
        // messages_per_second must equal iterations / elapsed seconds.
        let expected_mps = report.iterations as f64 / (report.elapsed_ms / 1000.0);
        assert!((report.messages_per_second - expected_mps).abs() < 1e-6);
        // kbytes_per_second must equal bytes / 1024 / elapsed seconds.
        let expected_kbps = report.bytes_received as f64 / 1024.0 / (report.elapsed_ms / 1000.0);
        assert!((report.kbytes_per_second() - expected_kbps).abs() < 1e-6);
    }
}
