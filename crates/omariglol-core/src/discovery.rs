//! Instrument discovery, mirroring `lxi discover` and `lxi discover --mdns`.
//!
//! Two independent mechanisms are supported:
//!
//! * **VXI-11 broadcast** — a `PMAP_DUMP` RPC call over UDP to the broadcast
//!   address on port 111. Any host that answers and has the VXI-11 core
//!   program registered is an LXI instrument. Subnet-local by design.
//! * **mDNS / DNS-SD** — browse `_lxi._tcp`, `_scpi-raw._tcp` and
//!   `_vxi-11._tcp` on 224.0.0.251. Finds instruments that advertise
//!   themselves and can cross a routed boundary.
//!
//! Results from both are merged by IP address, preferring whichever record
//! carries more information.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use crate::device::Device;
use crate::error::Result;

/// The IPv4 all-hosts multicast group used by mDNS.
pub const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
/// The mDNS port.
pub const MDNS_PORT: u16 = 5353;

/// DNS-SD service types advertised by LXI instruments.
pub const MDNS_SERVICES: [&str; 3] = [
    "_lxi._tcp.local",
    "_scpi-raw._tcp.local",
    "_vxi-11._tcp.local",
];

/// The default discovery window, matching lxi-tools' 3 second default.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// Discover instruments, optionally including mDNS.
pub fn discover(timeout: Duration, use_mdns: bool) -> Result<Vec<Device>> {
    let _ = (timeout, use_mdns);
    todo!("run both mechanisms and merge the results by IP")
}

/// Broadcast a `PMAP_DUMP` to the local subnet and collect LXI instruments.
pub fn discover_vxi11(timeout: Duration) -> Result<Vec<Device>> {
    let _ = timeout;
    todo!("broadcast PMAP_DUMP over UDP and keep hosts exposing DEVICE_CORE")
}

/// Browse the LXI-related mDNS service types and return the instruments found.
pub fn discover_mdns(timeout: Duration) -> Result<Vec<Device>> {
    let _ = timeout;
    todo!("browse _lxi/_scpi-raw/_vxi-11 and correlate PTR/SRV/A records")
}

/// Decode a DNS name at `pos`, following compression pointers.
///
/// Returns the name and the offset just past it. Split out because name
/// compression is the part of mDNS most likely to be subtly wrong, and it is
/// unit-testable in isolation.
pub(crate) fn read_name(data: &[u8], pos: usize) -> Option<(String, usize)> {
    let _ = (data, pos);
    todo!("decode a possibly-compressed DNS name")
}

/// Lower-case a DNS name for use as a map key.
pub(crate) fn normalise(name: &str) -> String {
    let _ = name;
    todo!("normalise a DNS name")
}

/// `Rigol DS1054Z (abc)._scpi-raw._tcp.local` -> `Rigol DS1054Z (abc)`
pub(crate) fn strip_service_suffix(instance: &str) -> String {
    let _ = instance;
    todo!("trim the service type from an mDNS instance name")
}

/// Drop the `.local` suffix from an mDNS hostname.
pub(crate) fn strip_domain(host: &str) -> String {
    let _ = host;
    todo!("trim the .local suffix")
}

/// A device discovered via mDNS, before correlation into a [`Device`].
#[derive(Debug, Clone)]
pub struct MdnsRecords {
    /// Instance names from PTR records.
    pub instances: Vec<String>,
    /// SRV records: instance name -> (target host, port).
    pub srv: std::collections::HashMap<String, (String, u16)>,
    /// A/AAAA records: host name -> address.
    pub addresses: std::collections::HashMap<String, IpAddr>,
}

impl MdnsRecords {
    /// Fold one received DNS packet into the accumulator.
    pub fn ingest(&mut self, packet: &[u8]) {
        let _ = packet;
        todo!("parse a DNS packet and accumulate PTR/SRV/A records")
    }

    /// Correlate instance -> SRV -> address into device entries.
    pub fn into_devices(self) -> Vec<Device> {
        todo!("resolve instances through SRV and A records")
    }
}
