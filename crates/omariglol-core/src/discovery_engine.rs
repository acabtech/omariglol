//! The discovery engine: multi-strategy discovery with a TTL cache
//! (discovery subsystem).
//!
//! The engine composes the low-level strategies in [`crate::discovery`] and
//! [`crate::netconfig`] into a single, cache-backed operation:
//!
//! 1. Detect the network configuration (subnet, gateway, VLAN, link-local).
//! 2. Run the broadcast and mDNS strategies, which find instruments that
//!    advertise themselves.
//! 3. Scan the local subnet (and any extra hosts) with direct SCPI `*IDN?`
//!    probes over TCP, in parallel.
//! 4. Fold everything into a [`DiscoveryCache`] with a configurable TTL.
//!
//! ## Incremental re-discovery
//!
//! When `incremental` is set, hosts whose cache entry is still fresh are
//! skipped, so a repeated scan only probes what is new or expired. This is what
//! keeps a periodic re-discovery from flooding the subnet with broadcast and
//! connect traffic.
//!
//! ## Concurrency
//!
//! The cache is behind an `Arc<Mutex<…>>`, so several workspaces can share one
//! engine and its cache. The engine itself is stateless apart from the cache,
//! and every strategy is bounded by a timeout, so a run always terminates.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::device::Device;
use crate::discovery::{self, ProbeResult, DEFAULT_SCAN_CONCURRENCY, PROBE_TIMEOUT, SCPI_PROBE_PORTS};
use crate::driver::InstrumentDriver;
use crate::netconfig::{self, NetworkConfig};
use crate::workspace::now_millis;

/// The default cache TTL, in seconds.
pub const DEFAULT_TTL_SECS: u64 = 300;

/// The default cap on hosts scanned in one run, so a `/8` cannot run forever.
pub const DEFAULT_MAX_HOSTS: usize = 1024;

/// Cached metadata about a discovered instrument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedInstrument {
    /// The discovered device.
    pub device: Device,
    /// The `*IDN?` string, when the instrument answered a probe.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub idn: String,
    /// The vendor, parsed from the IDN.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    /// The model, parsed from the IDN.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// The serial number, parsed from the IDN.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub serial: String,
    /// The named operations the instrument's driver exposes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// The VLAN the instrument was found on, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u16>,
    /// When the entry was recorded, in milliseconds since the epoch.
    pub discovered_ms: u64,
    /// The entry's time-to-live, in seconds.
    pub ttl_secs: u64,
}

impl CachedInstrument {
    /// Build a cache entry from a TCP probe result.
    pub fn from_probe(result: &ProbeResult, vlan: Option<u16>, ttl_secs: u64) -> Self {
        let driver = InstrumentDriver::from_idn(&result.idn);
        CachedInstrument {
            device: result.device.clone(),
            idn: result.idn.clone(),
            vendor: driver.vendor,
            model: driver.model,
            serial: idn_serial(&result.idn),
            capabilities: driver
                .commands
                .iter()
                .map(|command| command.name.clone())
                .collect(),
            vlan,
            discovered_ms: now_millis(),
            ttl_secs,
        }
    }

    /// Build a cache entry from a device found by broadcast or mDNS.
    ///
    /// These strategies do not return an IDN, so the metadata is left empty
    /// until a later probe fills it in.
    pub fn from_device(device: Device, vlan: Option<u16>, ttl_secs: u64) -> Self {
        CachedInstrument {
            device,
            idn: String::new(),
            vendor: String::new(),
            model: String::new(),
            serial: String::new(),
            capabilities: Vec::new(),
            vlan,
            discovered_ms: now_millis(),
            ttl_secs,
        }
    }

    /// True when the entry is still within its TTL.
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.discovered_ms) < self.ttl_secs.saturating_mul(1000)
    }

    /// True when the model contains `needle` (case-insensitive).
    pub fn matches_model(&self, needle: &str) -> bool {
        self.model.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
    }

    /// True when the entry exposes a named capability.
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    /// The instrument's address.
    pub fn address(&self) -> IpAddr {
        self.device.address
    }
}

/// Extract the serial number (the third comma-separated field) from an IDN.
fn idn_serial(idn: &str) -> String {
    idn.split(',')
        .nth(2)
        .map(|serial| serial.trim().to_string())
        .unwrap_or_default()
}

/// A TTL cache of discovered instruments, keyed by address.
#[derive(Debug, Clone)]
pub struct DiscoveryCache {
    entries: HashMap<IpAddr, CachedInstrument>,
    default_ttl_secs: u64,
}

impl DiscoveryCache {
    /// A cache with the given default TTL.
    pub fn new(default_ttl_secs: u64) -> Self {
        DiscoveryCache {
            entries: HashMap::new(),
            default_ttl_secs,
        }
    }

    /// The default TTL, in seconds.
    pub fn default_ttl_secs(&self) -> u64 {
        self.default_ttl_secs
    }

    /// Insert or replace an entry.
    pub fn insert(&mut self, entry: CachedInstrument) {
        self.entries.insert(entry.device.address, entry);
    }

    /// The fresh entry for an address, if any.
    pub fn get(&self, address: IpAddr) -> Option<&CachedInstrument> {
        self.entries
            .get(&address)
            .filter(|entry| entry.is_fresh(now_millis()))
    }

    /// The entry for an address regardless of freshness.
    pub fn get_any(&self, address: IpAddr) -> Option<&CachedInstrument> {
        self.entries.get(&address)
    }

    /// Every fresh entry.
    pub fn fresh(&self) -> Vec<&CachedInstrument> {
        let now = now_millis();
        self.entries
            .values()
            .filter(|entry| entry.is_fresh(now))
            .collect()
    }

    /// Every entry, fresh or not.
    pub fn all(&self) -> Vec<&CachedInstrument> {
        self.entries.values().collect()
    }

    /// The addresses whose entries are still fresh.
    pub fn fresh_addresses(&self) -> Vec<IpAddr> {
        let now = now_millis();
        self.entries
            .values()
            .filter(|entry| entry.is_fresh(now))
            .map(|entry| entry.device.address)
            .collect()
    }

    /// Remove expired entries, returning how many were dropped.
    pub fn evict_expired(&mut self) -> usize {
        let now = now_millis();
        let before = self.entries.len();
        self.entries.retain(|_, entry| entry.is_fresh(now));
        before - self.entries.len()
    }

    /// Entries whose model contains `needle`.
    pub fn find_by_model(&self, needle: &str) -> Vec<&CachedInstrument> {
        self.entries
            .values()
            .filter(|entry| entry.matches_model(needle))
            .collect()
    }

    /// Entries that expose a named capability.
    pub fn find_by_capability(&self, capability: &str) -> Vec<&CachedInstrument> {
        self.entries
            .values()
            .filter(|entry| entry.has_capability(capability))
            .collect()
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove every entry.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for DiscoveryCache {
    fn default() -> Self {
        DiscoveryCache::new(DEFAULT_TTL_SECS)
    }
}

/// Which strategies a discovery run uses.
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// Run the VXI-11 broadcast strategy.
    pub use_broadcast: bool,
    /// Run the mDNS/DNS-SD strategy.
    pub use_mdns: bool,
    /// Scan the local subnet with direct TCP probes.
    pub scan_subnet: bool,
    /// Additional hosts to probe (static addresses, other subnets).
    pub extra_hosts: Vec<IpAddr>,
    /// The ports to probe.
    pub ports: Vec<u16>,
    /// The per-host probe timeout.
    pub probe_timeout: Duration,
    /// The broadcast/mDNS listen window.
    pub listen_timeout: Duration,
    /// The number of scan worker threads.
    pub concurrency: usize,
    /// The TTL applied to new cache entries.
    pub ttl_secs: u64,
    /// Skip hosts whose cache entry is still fresh.
    pub incremental: bool,
    /// The maximum number of hosts to scan.
    pub max_hosts: usize,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        DiscoveryOptions {
            use_broadcast: true,
            use_mdns: true,
            scan_subnet: true,
            extra_hosts: Vec::new(),
            ports: SCPI_PROBE_PORTS.to_vec(),
            probe_timeout: PROBE_TIMEOUT,
            listen_timeout: Duration::from_secs(3),
            concurrency: DEFAULT_SCAN_CONCURRENCY,
            ttl_secs: DEFAULT_TTL_SECS,
            incremental: true,
            max_hosts: DEFAULT_MAX_HOSTS,
        }
    }
}

/// A structured event emitted during a discovery run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DiscoveryEvent {
    /// The run started, listing the strategies in use.
    Started { strategies: Vec<String> },
    /// The network configuration was detected.
    NetworkDetected { config: NetworkConfig },
    /// An instrument was found (or refreshed).
    InstrumentFound { instrument: CachedInstrument },
    /// Scan progress.
    Progress { probed: usize, total: usize },
    /// The run finished.
    Finished { found: usize, elapsed_ms: f64 },
}

/// The outcome of a discovery run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryReport {
    /// The detected network configuration.
    pub config: NetworkConfig,
    /// The instruments found or refreshed, fresh first.
    pub found: Vec<CachedInstrument>,
    /// How many hosts were probed.
    pub probed_hosts: usize,
    /// How many hosts were skipped because their cache entry was fresh.
    pub skipped_hosts: usize,
    /// How long the run took, in milliseconds.
    pub elapsed_ms: f64,
    /// The strategies that ran.
    pub strategies: Vec<String>,
}

/// The discovery engine, owning a shared cache.
#[derive(Clone)]
pub struct DiscoveryEngine {
    cache: Arc<Mutex<DiscoveryCache>>,
}

impl DiscoveryEngine {
    /// An engine with a fresh cache and the given default TTL.
    pub fn new(ttl_secs: u64) -> Self {
        DiscoveryEngine {
            cache: Arc::new(Mutex::new(DiscoveryCache::new(ttl_secs))),
        }
    }

    /// An engine sharing an existing cache, so several workspaces see the same
    /// results.
    pub fn with_cache(cache: Arc<Mutex<DiscoveryCache>>) -> Self {
        DiscoveryEngine { cache }
    }

    /// The shared cache.
    pub fn cache(&self) -> Arc<Mutex<DiscoveryCache>> {
        Arc::clone(&self.cache)
    }

    /// Run discovery, emitting structured events as it goes.
    ///
    /// The callback is invoked from the calling thread and from scan worker
    /// threads, so it must be `Send + Sync`.
    pub fn run(
        &self,
        options: &DiscoveryOptions,
        on_event: impl Fn(DiscoveryEvent) + Send + Sync + 'static,
    ) -> DiscoveryReport {
        let start = Instant::now();
        let on_event = Arc::new(on_event);

        let mut strategies = Vec::new();
        if options.use_broadcast {
            strategies.push("vxi11-broadcast".to_string());
        }
        if options.use_mdns {
            strategies.push("mdns".to_string());
        }
        if options.scan_subnet {
            strategies.push("subnet-scan".to_string());
        }
        if !options.extra_hosts.is_empty() {
            strategies.push("static-hosts".to_string());
        }
        on_event(DiscoveryEvent::Started {
            strategies: strategies.clone(),
        });

        // 1. Detect the network configuration.
        let config = netconfig::detect();
        on_event(DiscoveryEvent::NetworkDetected {
            config: config.clone(),
        });
        let vlan = config.primary.as_ref().and_then(|interface| interface.vlan);

        // 2. Broadcast and mDNS, which find self-advertising instruments.
        //
        // Each listens for a fixed window, so they run on their own background
        // threads while the subnet scan proceeds on this one. All three overlap,
        // which keeps a full scan within the performance budget instead of
        // paying each listen window and the scan back to back.
        let listen_timeout = options.listen_timeout;
        let broadcast_handle = options.use_broadcast.then(|| {
            std::thread::spawn(move || {
                discovery::discover_vxi11(listen_timeout).unwrap_or_default()
            })
        });
        let mdns_handle = options.use_mdns.then(|| {
            std::thread::spawn(move || {
                discovery::discover_mdns(listen_timeout).unwrap_or_default()
            })
        });

        // 3. Build the host list for direct probing.
        let mut hosts: Vec<IpAddr> = Vec::new();
        if options.scan_subnet {
            for host in config.candidate_hosts(options.max_hosts) {
                hosts.push(IpAddr::V4(host));
            }
        }
        hosts.extend(options.extra_hosts.iter().copied());

        // 4. Incremental: skip hosts whose cache entry is still fresh.
        let mut skipped = 0usize;
        if options.incremental {
            let fresh = self.cache.lock().unwrap().fresh_addresses();
            let before = hosts.len();
            hosts.retain(|host| !fresh.contains(host));
            skipped = before - hosts.len();
        }

        let total = hosts.len();
        let probed = Arc::new(Mutex::new(0usize));
        let cache = self.cache.clone();
        let on_found = {
            let on_event = Arc::clone(&on_event);
            let probed = Arc::clone(&probed);
            let ttl = options.ttl_secs;
            move |result: &ProbeResult| {
                let entry = CachedInstrument::from_probe(result, vlan, ttl);
                cache.lock().unwrap().insert(entry.clone());
                on_event(DiscoveryEvent::InstrumentFound { instrument: entry });
                let mut count = probed.lock().unwrap();
                *count += 1;
                on_event(DiscoveryEvent::Progress {
                    probed: *count,
                    total,
                });
            }
        };

        let ipv4_hosts: Vec<std::net::Ipv4Addr> = hosts
            .iter()
            .filter_map(|host| match host {
                IpAddr::V4(ip) => Some(*ip),
                IpAddr::V6(_) => None,
            })
            .collect();

        discovery::scan_hosts(
            &ipv4_hosts,
            &options.ports,
            options.probe_timeout,
            options.concurrency,
            on_found,
        );

        // 4b. Fold in the broadcast/mDNS results once they have finished.
        let mut advertised: Vec<Device> = Vec::new();
        if let Some(handle) = broadcast_handle
            && let Ok(devices) = handle.join()
        {
            advertised = discovery::merge_devices(advertised, devices);
        }
        if let Some(handle) = mdns_handle
            && let Ok(devices) = handle.join()
        {
            advertised = discovery::merge_devices(advertised, devices);
        }
        if !advertised.is_empty() {
            let mut cache = self.cache.lock().unwrap();
            for device in advertised {
                let entry = CachedInstrument::from_device(device, vlan, options.ttl_secs);
                on_event(DiscoveryEvent::InstrumentFound {
                    instrument: entry.clone(),
                });
                cache.insert(entry);
            }
        }

        // 5. Evict expired entries and collect the fresh set.
        let found = {
            let mut cache = self.cache.lock().unwrap();
            cache.evict_expired();
            cache
                .fresh()
                .into_iter()
                .cloned()
                .collect::<Vec<CachedInstrument>>()
        };

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        on_event(DiscoveryEvent::Finished {
            found: found.len(),
            elapsed_ms,
        });

        DiscoveryReport {
            config,
            found,
            probed_hosts: total,
            skipped_hosts: skipped,
            elapsed_ms,
            strategies,
        }
    }
}

impl Default for DiscoveryEngine {
    fn default() -> Self {
        DiscoveryEngine::new(DEFAULT_TTL_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DiscoveryMethod;
    use std::net::Ipv4Addr;

    fn probe(address: &str, idn: &str) -> ProbeResult {
        let mut device = Device::manual(address.parse().unwrap());
        device.method = DiscoveryMethod::ScpiTcp;
        device.raw_port = Some(5025);
        ProbeResult {
            device,
            idn: idn.to_string(),
        }
    }

    #[test]
    fn test_cached_instrument_from_probe() {
        let entry = CachedInstrument::from_probe(
            &probe("192.168.1.50", "RIGOL TECHNOLOGIES,DS1054Z,DS1ZA123,1.0"),
            Some(100),
            300,
        );
        assert_eq!(entry.vendor, "RIGOL TECHNOLOGIES");
        assert_eq!(entry.model, "DS1054Z");
        assert_eq!(entry.serial, "DS1ZA123");
        assert_eq!(entry.vlan, Some(100));
        assert!(entry.has_capability("run"));
        assert!(entry.has_capability("measure_vpp"));
        assert!(entry.matches_model("ds1054"));
        assert!(!entry.matches_model("keysight"));
    }

    #[test]
    fn test_cached_instrument_freshness() {
        let mut entry = CachedInstrument::from_probe(
            &probe("192.168.1.50", "RIGOL TECHNOLOGIES,DS1054Z,SN,1.0"),
            None,
            10,
        );
        let now = entry.discovered_ms;
        assert!(entry.is_fresh(now));
        assert!(entry.is_fresh(now + 9_000));
        assert!(!entry.is_fresh(now + 11_000));
        entry.ttl_secs = 0;
        assert!(!entry.is_fresh(now));
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = DiscoveryCache::new(300);
        let entry = CachedInstrument::from_probe(
            &probe("192.168.1.50", "RIGOL TECHNOLOGIES,DS1054Z,SN,1.0"),
            None,
            300,
        );
        let address = entry.device.address;
        cache.insert(entry);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(address).is_some());
        assert!(cache.get("10.0.0.1".parse().unwrap()).is_none());
    }

    #[test]
    fn test_cache_find_by_model_and_capability() {
        let mut cache = DiscoveryCache::new(300);
        cache.insert(CachedInstrument::from_probe(
            &probe("192.168.1.50", "RIGOL TECHNOLOGIES,DS1054Z,SN,1.0"),
            None,
            300,
        ));
        cache.insert(CachedInstrument::from_probe(
            &probe("192.168.1.51", "KEYSIGHT TECHNOLOGIES,34401A,SN,1.0"),
            None,
            300,
        ));
        assert_eq!(cache.find_by_model("ds1054").len(), 1);
        assert_eq!(cache.find_by_model("34401").len(), 1);
        assert_eq!(cache.find_by_capability("run").len(), 1);
        assert_eq!(cache.find_by_capability("measure_voltage").len(), 1);
    }

    #[test]
    fn test_cache_evict_expired() {
        let mut cache = DiscoveryCache::new(300);
        let mut entry = CachedInstrument::from_probe(
            &probe("192.168.1.50", "RIGOL TECHNOLOGIES,DS1054Z,SN,1.0"),
            None,
            300,
        );
        // Backdate the entry so it is expired.
        entry.discovered_ms = 0;
        entry.ttl_secs = 1;
        cache.insert(entry);
        assert_eq!(cache.evict_expired(), 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = DiscoveryCache::new(300);
        cache.insert(CachedInstrument::from_probe(
            &probe("192.168.1.50", "RIGOL TECHNOLOGIES,DS1054Z,SN,1.0"),
            None,
            300,
        ));
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_engine_run_with_no_strategies() {
        let engine = DiscoveryEngine::new(300);
        let options = DiscoveryOptions {
            use_broadcast: false,
            use_mdns: false,
            scan_subnet: false,
            extra_hosts: Vec::new(),
            ..Default::default()
        };
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let report = engine.run(&options, move |event| {
            sink.lock().unwrap().push(event);
        });
        assert!(report.strategies.is_empty());
        assert_eq!(report.probed_hosts, 0);
        let events = events.lock().unwrap();
        // Started, NetworkDetected and Finished are always emitted.
        assert!(events.len() >= 3);
    }

    #[test]
    fn test_engine_probes_static_hosts() {
        let engine = DiscoveryEngine::new(300);
        // A loopback address with no instrument: the probe must fail quietly.
        let options = DiscoveryOptions {
            use_broadcast: false,
            use_mdns: false,
            scan_subnet: false,
            extra_hosts: vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))],
            probe_timeout: Duration::from_millis(50),
            concurrency: 1,
            ..Default::default()
        };
        let report = engine.run(&options, |_| {});
        assert_eq!(report.probed_hosts, 1);
        assert!(report.found.is_empty());
    }

    #[test]
    fn test_engine_incremental_skips_fresh() {
        let engine = DiscoveryEngine::new(300);
        // Seed the cache with a fresh entry for the host we are about to scan.
        {
            let cache_handle = engine.cache();
            let mut cache = cache_handle.lock().unwrap();
            cache.insert(CachedInstrument::from_probe(
                &probe("127.0.0.1", "RIGOL TECHNOLOGIES,DS1054Z,SN,1.0"),
                None,
                300,
            ));
        }
        let options = DiscoveryOptions {
            use_broadcast: false,
            use_mdns: false,
            scan_subnet: false,
            extra_hosts: vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))],
            probe_timeout: Duration::from_millis(50),
            concurrency: 1,
            incremental: true,
            ..Default::default()
        };
        let report = engine.run(&options, |_| {});
        assert_eq!(report.probed_hosts, 0);
        assert_eq!(report.skipped_hosts, 1);
    }

    #[test]
    fn test_engine_shared_cache() {
        let cache = Arc::new(Mutex::new(DiscoveryCache::new(300)));
        let a = DiscoveryEngine::with_cache(cache.clone());
        let b = DiscoveryEngine::with_cache(cache.clone());
        a.cache().lock().unwrap().insert(CachedInstrument::from_probe(
            &probe("192.168.1.50", "RIGOL TECHNOLOGIES,DS1054Z,SN,1.0"),
            None,
            300,
        ));
        assert_eq!(b.cache().lock().unwrap().len(), 1);
    }
}
