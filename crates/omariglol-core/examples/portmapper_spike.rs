//! M0 spike: portmapper-only LXI detection without a VXI-11 link.
//!
//! This program exists to answer one question: can discovery identify an LXI
//! instrument by talking only to its portmapper, so that repeated discovery
//! never creates and destroys VXI-11 links? Some instruments (the Rigol
//! DS1000Z used during development) became unstable when discovery opened a
//! link per probe.
//!
//! It issues only:
//!
//! * TCP `PMAP_GETPORT` for `DEVICE_CORE`
//! * UDP `PMAP_GETPORT` for `DEVICE_CORE`
//! * TCP `PMAP_DUMP` (periodically)
//!
//! It deliberately never calls `create_link`, `device_write`, `device_read` or
//! `destroy_link`.
//!
//! Usage:
//!
//! ```sh
//! cargo run -p omariglol-core --example portmapper_spike -- \
//!     192.168.100.36 30 100
//! ```
//!
//! Arguments: `HOST [ITERATIONS] [INTERVAL_MS]` (defaults: the DS1054Z address,
//! 30 iterations, 100 ms between iterations).

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use omariglol_core::transport::{rpc, vxi11};

/// Per-request deadline, matching the discovery fallback's budget.
const TIMEOUT: Duration = Duration::from_millis(750);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host: IpAddr = args
        .first()
        .map(String::as_str)
        .unwrap_or("192.168.100.36")
        .parse()?;
    let iterations: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let interval_ms: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);

    println!("portmapper-only spike");
    println!("  host:       {host}");
    println!("  iterations: {iterations}");
    println!("  interval:   {interval_ms} ms");
    println!("  timeout:    {TIMEOUT:?}");
    println!("  no VXI-11 link is created at any point\n");

    let mut tcp_latency = Vec::new();
    let mut udp_latency = Vec::new();
    let mut dump_latency = Vec::new();
    let mut ports = BTreeSet::new();
    let mut tcp_ok = 0usize;
    let mut udp_ok = 0usize;
    let mut dump_ok = 0usize;
    let mut failures = 0usize;

    for iteration in 0..iterations {
        let start = Instant::now();
        match rpc::pmap_getport(
            host,
            vxi11::DEVICE_CORE,
            vxi11::DEVICE_CORE_VERSION,
            rpc::IPPROTO_TCP,
            TIMEOUT,
        ) {
            Ok(Some(port)) => {
                tcp_ok += 1;
                ports.insert(port);
                tcp_latency.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            Ok(None) => {
                failures += 1;
                println!("  #{iteration:03} tcp getport: DEVICE_CORE not registered");
            }
            Err(err) => {
                failures += 1;
                println!("  #{iteration:03} tcp getport: {err}");
            }
        }

        let start = Instant::now();
        match rpc::pmap_getport_udp(
            host,
            vxi11::DEVICE_CORE,
            vxi11::DEVICE_CORE_VERSION,
            rpc::IPPROTO_TCP,
            TIMEOUT,
        ) {
            Ok(Some(_)) => {
                udp_ok += 1;
                udp_latency.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            Ok(None) => {
                // A portmapper may answer UDP with 0 while TCP still works.
            }
            Err(err) => println!("  #{iteration:03} udp getport: {err}"),
        }

        // Exercise the full table now and then; discovery's broadcast path uses
        // DUMP, so it belongs in the same stability experiment.
        if iteration % 10 == 0 {
            let start = Instant::now();
            match rpc::pmap_dump(host, TIMEOUT) {
                Ok(mappings) => {
                    let has_core = mappings
                        .iter()
                        .any(|m| m.prog == vxi11::DEVICE_CORE && m.prot == rpc::IPPROTO_TCP);
                    if has_core {
                        dump_ok += 1;
                        dump_latency.push(start.elapsed().as_secs_f64() * 1000.0);
                    } else {
                        failures += 1;
                        println!("  #{iteration:03} tcp dump: no DEVICE_CORE mapping");
                    }
                }
                Err(err) => {
                    failures += 1;
                    println!("  #{iteration:03} tcp dump: {err}");
                }
            }
        }

        std::thread::sleep(Duration::from_millis(interval_ms));
    }

    let dump_attempts = iterations.div_ceil(10);
    println!("\nresults");
    println!(
        "  tcp getport: {tcp_ok}/{iterations} ok, {}",
        describe(&mut tcp_latency)
    );
    println!(
        "  udp getport: {udp_ok}/{iterations} ok, {}",
        describe(&mut udp_latency)
    );
    println!(
        "  tcp dump:    {dump_ok}/{dump_attempts} ok, {}",
        describe(&mut dump_latency)
    );
    let port_list: Vec<String> = ports.iter().map(u16::to_string).collect();
    println!("  DEVICE_CORE ports observed: [{}]", port_list.join(", "));
    println!("  failures: {failures}");
    println!(
        "  verdict: {}",
        if failures == 0 && tcp_ok == iterations {
            "stable (portmapper-only detection held for the whole run)"
        } else {
            "flaky (review the failures above)"
        }
    );

    Ok(())
}

/// Sort latencies in place and render a compact min/p50/p95/max summary.
fn describe(samples: &mut [f64]) -> String {
    if samples.is_empty() {
        return "no successful samples".to_string();
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let p = |fraction: f64| {
        let index = ((samples.len() - 1) as f64 * fraction).round() as usize;
        samples[index]
    };
    format!(
        "min {:.1} ms, p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        samples[0],
        p(0.50),
        p(0.95),
        p(1.0)
    )
}
