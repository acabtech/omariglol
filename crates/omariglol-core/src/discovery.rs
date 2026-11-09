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

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::device::{Device, DiscoveryMethod};
use crate::error::Result;
use crate::transport::{rpc, vxi11};

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

// DNS record types we care about.
const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_AAAA: u16 = 28;
const TYPE_SRV: u16 = 33;

/// Upper bound on compression-pointer hops, so a hostile packet cannot spin
/// forever. A legitimate name is at most 255 bytes, so this is generous.
const MAX_POINTER_JUMPS: usize = 128;

/// Discover instruments, optionally including mDNS.
///
/// The two mechanisms are independent: a failure in one (for example a host
/// with no route to the broadcast address) does not prevent the other from
/// contributing results.
pub fn discover(timeout: Duration, use_mdns: bool) -> Result<Vec<Device>> {
    let mut devices = discover_vxi11(timeout).unwrap_or_default();
    if use_mdns {
        if let Ok(mdns) = discover_mdns(timeout) {
            devices = merge_devices(devices, mdns);
        }
    }
    Ok(devices)
}

/// Broadcast a `PMAP_DUMP` to the local subnet and collect LXI instruments.
///
/// A host is an instrument when its portmapper lists the VXI-11 `DEVICE_CORE`
/// program on TCP; the registered port becomes the device's `vxi11_port`.
pub fn discover_vxi11(timeout: Duration) -> Result<Vec<Device>> {
    let hosts = rpc::pmap_dump_udp_hosts(timeout)?;
    let mut devices = Vec::new();
    for (ip, mappings) in hosts {
        let vxi11_port = mappings
            .iter()
            .find(|m| m.prog == vxi11::DEVICE_CORE && m.prot == rpc::IPPROTO_TCP)
            .map(|m| m.port);
        if let Some(port) = vxi11_port {
            let mut device = Device::manual(ip);
            device.method = DiscoveryMethod::Vxi11Broadcast;
            device.vxi11_port = Some(port);
            devices.push(device);
        }
    }
    Ok(devices)
}

/// Browse the LXI-related mDNS service types and return the instruments found.
///
/// The query is sent from an ephemeral port, which per RFC 6762 makes
/// responders answer by unicast to that port — so no multicast group
/// membership (and no fight over port 5353) is required.
pub fn discover_mdns(timeout: Duration) -> Result<Vec<Device>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    let query = build_query(&MDNS_SERVICES);
    socket.send_to(&query, SocketAddr::new(IpAddr::V4(MDNS_GROUP), MDNS_PORT))?;

    let mut records = MdnsRecords::default();
    let mut buf = vec![0u8; 9000];
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(remaining))?;
        match socket.recv_from(&mut buf) {
            Ok((n, _src)) => records.ingest(&buf[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(_) => break,
        }
    }
    Ok(records.into_devices())
}

/// Build a DNS-SD query packet asking for PTR records for each service.
///
/// The class field carries the unicast-response (QU) bit so responders reply
/// directly to our ephemeral source port.
fn build_query(services: &[&str]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u16.to_be_bytes()); // transaction id
    buf.extend_from_slice(&0u16.to_be_bytes()); // flags: standard query
    buf.extend_from_slice(&(services.len() as u16).to_be_bytes()); // QDCOUNT
    buf.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    buf.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    buf.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    for service in services {
        buf.extend_from_slice(&encode_name(service));
        buf.extend_from_slice(&TYPE_PTR.to_be_bytes());
        buf.extend_from_slice(&0x8001u16.to_be_bytes()); // IN + QU
    }
    buf
}

/// Encode a dotted DNS name as a sequence of length-prefixed labels.
fn encode_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out
}

/// Decode a DNS name at `pos`, following compression pointers.
///
/// Returns the name and the offset just past it. Split out because name
/// compression is the part of mDNS most likely to be subtly wrong, and it is
/// unit-testable in isolation.
///
/// Returns `None` on any malformed input: a truncated label, a pointer past the
/// end of the packet, a reserved label type, or a pointer loop.
pub(crate) fn read_name(data: &[u8], pos: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut cursor = pos;
    // The offset just past the name in the original stream. Set the first time
    // we either hit the root label or follow a pointer.
    let mut end: Option<usize> = None;
    let mut jumps = 0usize;

    loop {
        let len = *data.get(cursor)?;
        if len == 0 {
            cursor += 1;
            if end.is_none() {
                end = Some(cursor);
            }
            break;
        }
        if len & 0xC0 == 0xC0 {
            // Compression pointer: the low 14 bits are an offset into the packet.
            let second = *data.get(cursor + 1)?;
            let target = (((len as usize) & 0x3F) << 8) | second as usize;
            if end.is_none() {
                end = Some(cursor + 2);
            }
            jumps += 1;
            if jumps > MAX_POINTER_JUMPS || target >= data.len() {
                return None;
            }
            cursor = target;
            continue;
        }
        if len & 0xC0 != 0 {
            // Reserved label type (0x40 / 0x80).
            return None;
        }
        let start = cursor + 1;
        let stop = start + len as usize;
        if stop > data.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&data[start..stop]).into_owned());
        cursor = stop;
    }

    Some((labels.join("."), end.unwrap_or(cursor)))
}

/// Lower-case a DNS name for use as a map key.
pub(crate) fn normalise(name: &str) -> String {
    name.trim_end_matches('.').to_ascii_lowercase()
}

/// `Rigol DS1054Z (abc)._scpi-raw._tcp.local` -> `Rigol DS1054Z (abc)`
pub(crate) fn strip_service_suffix(instance: &str) -> String {
    let lower = instance.to_ascii_lowercase();
    for service in MDNS_SERVICES {
        if let Some(idx) = lower.find(service) {
            // Drop the dot that separates the instance from the service type.
            let end = if idx > 0 && instance.as_bytes()[idx - 1] == b'.' {
                idx - 1
            } else {
                idx
            };
            return instance[..end].to_string();
        }
    }
    // Fallback for a service type we do not know about.
    match instance.find("._") {
        Some(idx) => instance[..idx].to_string(),
        None => instance.to_string(),
    }
}

/// Drop the `.local` suffix from an mDNS hostname.
pub(crate) fn strip_domain(host: &str) -> String {
    let trimmed = host.trim_end_matches('.');
    if trimmed.to_ascii_lowercase().ends_with(".local") {
        trimmed[..trimmed.len() - ".local".len()].to_string()
    } else {
        trimmed.to_string()
    }
}

/// A device discovered via mDNS, before correlation into a [`Device`].
#[derive(Debug, Clone, Default)]
pub struct MdnsRecords {
    /// Instance names from PTR records.
    pub instances: Vec<String>,
    /// SRV records: instance name -> (target host, port).
    pub srv: HashMap<String, (String, u16)>,
    /// A/AAAA records: host name -> address.
    pub addresses: HashMap<String, IpAddr>,
}

impl MdnsRecords {
    /// Fold one received DNS packet into the accumulator.
    ///
    /// Malformed packets are ignored rather than panicking: a hostile or
    /// truncated datagram must never take the process down.
    pub fn ingest(&mut self, packet: &[u8]) {
        if packet.len() < 12 {
            return;
        }
        let qdcount = u16::from_be_bytes([packet[4], packet[5]]) as usize;
        let ancount = u16::from_be_bytes([packet[6], packet[7]]) as usize;
        let nscount = u16::from_be_bytes([packet[8], packet[9]]) as usize;
        let arcount = u16::from_be_bytes([packet[10], packet[11]]) as usize;

        let mut pos = 12;
        // Skip the question section.
        for _ in 0..qdcount {
            match read_name(packet, pos) {
                Some((_, next)) => pos = next,
                None => return,
            }
            if pos + 4 > packet.len() {
                return;
            }
            pos += 4; // QTYPE + QCLASS
        }

        for _ in 0..(ancount + nscount + arcount) {
            let (name, next) = match read_name(packet, pos) {
                Some(v) => v,
                None => return,
            };
            pos = next;
            if pos + 10 > packet.len() {
                return;
            }
            let rtype = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
            let rdlength = u16::from_be_bytes([packet[pos + 8], packet[pos + 9]]) as usize;
            let rdata_start = pos + 10;
            let rdata_end = rdata_start + rdlength;
            if rdata_end > packet.len() {
                return;
            }
            self.ingest_record(&name, rtype, packet, rdata_start, rdata_end);
            pos = rdata_end;
        }
    }

    /// Fold a single resource record into the accumulator.
    fn ingest_record(
        &mut self,
        name: &str,
        rtype: u16,
        packet: &[u8],
        rdata_start: usize,
        rdata_end: usize,
    ) {
        match rtype {
            TYPE_A => {
                if rdata_end - rdata_start == 4 {
                    let ip = Ipv4Addr::new(
                        packet[rdata_start],
                        packet[rdata_start + 1],
                        packet[rdata_start + 2],
                        packet[rdata_start + 3],
                    );
                    self.addresses.insert(normalise(name), IpAddr::V4(ip));
                }
            }
            TYPE_AAAA => {
                if rdata_end - rdata_start == 16 {
                    let mut octets = [0u8; 16];
                    octets.copy_from_slice(&packet[rdata_start..rdata_end]);
                    self.addresses
                        .insert(normalise(name), IpAddr::V6(Ipv6Addr::from(octets)));
                }
            }
            TYPE_PTR => {
                if let Some((target, _)) = read_name(packet, rdata_start) {
                    let key = normalise(&target);
                    if !self.instances.iter().any(|i| normalise(i) == key) {
                        self.instances.push(target);
                    }
                }
            }
            TYPE_SRV if rdata_end - rdata_start >= 6 => {
                let port = u16::from_be_bytes([packet[rdata_start + 4], packet[rdata_start + 5]]);
                if let Some((target, _)) = read_name(packet, rdata_start + 6) {
                    self.srv.insert(normalise(name), (target, port));
                }
            }
            _ => {}
        }
    }

    /// Correlate instance -> SRV -> address into device entries.
    pub fn into_devices(self) -> Vec<Device> {
        let mut devices: Vec<Device> = Vec::new();
        for instance in &self.instances {
            let (target, port) = match self.srv.get(&normalise(instance)) {
                Some(entry) => entry.clone(),
                None => continue,
            };
            let ip = match self.addresses.get(&normalise(&target)) {
                Some(ip) => *ip,
                None => continue,
            };

            let lower = instance.to_ascii_lowercase();
            let mut device = Device::manual(ip);
            device.method = DiscoveryMethod::Mdns;
            device.mdns_name = Some(strip_service_suffix(instance));
            device.hostname = Some(strip_domain(&target));
            if lower.contains("_scpi-raw") {
                device.raw_port = Some(port);
            } else if lower.contains("_vxi-11") {
                device.vxi11_port = Some(port);
            }

            match devices.iter_mut().find(|d| d.address == ip) {
                Some(existing) => merge_into(existing, &device),
                None => devices.push(device),
            }
        }
        devices
    }
}

/// How much information a device record carries, used to pick the richer of
/// two records for the same IP.
fn richness(device: &Device) -> usize {
    device.hostname.is_some() as usize
        + device.vxi11_port.is_some() as usize
        + device.raw_port.is_some() as usize
        + device.mdns_name.is_some() as usize
}

/// Fill any gaps in `base` from `other`, without overwriting what `base` knows.
fn merge_into(base: &mut Device, other: &Device) {
    if base.hostname.is_none() {
        base.hostname = other.hostname.clone();
    }
    if base.vxi11_port.is_none() {
        base.vxi11_port = other.vxi11_port;
    }
    if base.raw_port.is_none() {
        base.raw_port = other.raw_port;
    }
    if base.mdns_name.is_none() {
        base.mdns_name = other.mdns_name.clone();
    }
    if base.method == DiscoveryMethod::Manual && other.method != DiscoveryMethod::Manual {
        base.method = other.method;
    }
}

/// Merge two device lists by IP address, preferring the richer record.
///
/// Public so a frontend can run the two discovery mechanisms separately and
/// emit results incrementally, then merge them the same way [`discover`] does.
pub fn merge_devices(mut base: Vec<Device>, extra: Vec<Device>) -> Vec<Device> {
    for device in extra {
        match base.iter_mut().find(|d| d.address == device.address) {
            Some(existing) => {
                if richness(&device) > richness(existing) {
                    let mut merged = device;
                    merge_into(&mut merged, &existing.clone());
                    *existing = merged;
                } else {
                    merge_into(existing, &device);
                }
            }
            None => base.push(device),
        }
    }
    base
}

// ---------------------------------------------------------------------------
// Direct SCPI probing over TCP
// ---------------------------------------------------------------------------

/// The SCPI raw-socket ports probed by default.
///
/// 5025 is the LXI standard; 5555 is Rigol's non-standard raw port. Both are
/// tried so a Rigol instrument is found without a vendor hint.
pub const SCPI_PROBE_PORTS: [u16; 2] = [5025, 5555];

/// The default per-host connect/read timeout for a TCP probe.
///
/// Kept short so a mostly-empty subnet scans quickly: a refused connection
/// fails immediately, and only a silent host pays the full timeout.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(200);

/// The per-host timeout for the portmapper probe.
///
/// The probe is one UDP datagram (`GETPORT`); a responsive instrument answers
/// in well under a millisecond, so a short deadline keeps the subnet scan fast
/// while still tolerating a slow instrument.
pub const VXI11_PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// The default number of worker threads a subnet scan uses.
///
/// A `/24` is 253 hosts; at 96 workers and a 200 ms timeout the worst case is
/// roughly `253 / 96 * 0.2 s * 2 ports ≈ 1.1 s`.
pub const DEFAULT_SCAN_CONCURRENCY: usize = 96;

/// A host that answered a SCPI `*IDN?` probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    /// The discovered device.
    pub device: Device,
    /// The `*IDN?` string the instrument returned.
    pub idn: String,
}

/// Send `*IDN?` to a host:port over TCP and return the response.
///
/// Returns `None` on any failure (refused, timed out, empty response), so a
/// scan over a mostly-empty subnet is cheap and quiet.
pub fn identify_tcp(address: IpAddr, port: u16, timeout: Duration) -> Option<String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let socket_addr = SocketAddr::new(address, port);
    let mut stream = TcpStream::connect_timeout(&socket_addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    stream.write_all(b"*IDN?\n").ok()?;
    stream.flush().ok()?;

    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf).ok()?;
    if n == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(&buf[..n]).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(text)
}

/// Probe one host on each candidate port, returning the first that answers.
pub fn probe_scpi_tcp(address: IpAddr, ports: &[u16], timeout: Duration) -> Option<ProbeResult> {
    for &port in ports {
        if let Some(idn) = identify_tcp(address, port, timeout) {
            let mut device = Device::manual(address);
            device.method = DiscoveryMethod::ScpiTcp;
            device.raw_port = Some(port);
            return Some(ProbeResult { device, idn });
        }
    }
    None
}

/// Detect an LXI instrument through its VXI-11 portmapper entry.
///
/// Some instruments serve VXI-11 and the portmapper while their raw SCPI
/// socket is disabled or owned by another interface (for example a scope whose
/// USB link takes precedence over LAN SCPI). Asking the portmapper for
/// `DEVICE_CORE` identifies the instrument without opening a VXI-11 link or a
/// raw socket, so discovery stays non-disruptive.
///
/// The query is sent over UDP: a `GETPORT` is one datagram and, on the
/// hardware measured for this spike, roughly two orders of magnitude faster
/// than a TCP round trip. RPCBIND is required to answer on both UDP and TCP;
/// using UDP alone keeps the subnet scan cheap and avoids a second deadline
/// per silent host.
pub fn probe_vxi11_tcp(address: IpAddr, timeout: Duration) -> Option<Device> {
    let port = rpc::pmap_getport_udp(
        address,
        vxi11::DEVICE_CORE,
        vxi11::DEVICE_CORE_VERSION,
        rpc::IPPROTO_TCP,
        timeout,
    )
    .ok()
    .flatten()?;
    let mut device = Device::manual(address);
    device.method = DiscoveryMethod::Vxi11Portmap;
    device.vxi11_port = Some(port);
    Some(device)
}

/// Scan a list of hosts in parallel, calling `on_found` as each answers and
/// `on_probe` once for every host after its probe completes.
///
/// `concurrency` worker threads pull hosts from a shared queue, so a `/24`
/// completes in roughly `hosts / concurrency * timeout` rather than the serial
/// product. `on_found` and `on_probe` are invoked from worker threads and must
/// be `Send + Sync`.
pub fn scan_hosts(
    hosts: &[Ipv4Addr],
    ports: &[u16],
    timeout: Duration,
    concurrency: usize,
    on_found: impl Fn(&ProbeResult) + Send + Sync + 'static,
    on_probe: impl Fn(&Ipv4Addr) + Send + Sync + 'static,
) -> Vec<ProbeResult> {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::thread;

    if hosts.is_empty() {
        return Vec::new();
    }

    let queue = Arc::new(Mutex::new(hosts.iter().copied().collect::<VecDeque<_>>()));
    let results = Arc::new(Mutex::new(Vec::new()));
    let on_found = Arc::new(on_found);
    let on_probe = Arc::new(on_probe);
    let ports = ports.to_vec();
    let workers = concurrency.max(1).min(hosts.len());

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        let on_found = Arc::clone(&on_found);
        let on_probe = Arc::clone(&on_probe);
        let ports = ports.clone();
        handles.push(thread::spawn(move || {
            loop {
                let host = queue.lock().unwrap().pop_front();
                let Some(host) = host else {
                    break;
                };
                let address = IpAddr::V4(host);
                // VXI-11 first: it is non-disruptive and also works when the
                // raw SCPI socket is disabled. Only fall back to the raw SCPI
                // probe when the instrument has no VXI-11 entry.
                let vxi11_timeout = timeout.max(VXI11_PROBE_TIMEOUT);
                let found = probe_vxi11_tcp(address, vxi11_timeout)
                    .map(|device| ProbeResult {
                        device,
                        idn: String::new(),
                    })
                    .or_else(|| probe_scpi_tcp(address, &ports, timeout));
                if let Some(result) = found {
                    on_found(&result);
                    results.lock().unwrap().push(result);
                }
                // Report the host itself, whether or not it answered, so
                // progress tracks probes rather than discoveries.
                on_probe(&host);
            }
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Arc::try_unwrap(results)
        .map(|mutex| mutex.into_inner().unwrap())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// Encode a resource record with an uncompressed owner name.
    fn record(name: &str, rtype: u16, rdata: &[u8]) -> Vec<u8> {
        let mut out = encode_name(name);
        out.extend_from_slice(&rtype.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes()); // class IN
        out.extend_from_slice(&120u32.to_be_bytes()); // TTL
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
        out
    }

    /// A captured-style mDNS response: PTR -> SRV -> A for one instrument.
    fn sample_packet() -> Vec<u8> {
        let instance = "Rigol DS1054Z (abc)._scpi-raw._tcp.local";
        let host = "rigol-abc.local";

        let mut packet = Vec::new();
        packet.extend_from_slice(&0u16.to_be_bytes()); // id
        packet.extend_from_slice(&0x8400u16.to_be_bytes()); // response, authoritative
        packet.extend_from_slice(&0u16.to_be_bytes()); // QDCOUNT
        packet.extend_from_slice(&3u16.to_be_bytes()); // ANCOUNT
        packet.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        packet.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT

        packet.extend_from_slice(&record(
            "_scpi-raw._tcp.local",
            TYPE_PTR,
            &encode_name(instance),
        ));

        let mut srv = Vec::new();
        srv.extend_from_slice(&0u16.to_be_bytes()); // priority
        srv.extend_from_slice(&0u16.to_be_bytes()); // weight
        srv.extend_from_slice(&5025u16.to_be_bytes()); // port
        srv.extend_from_slice(&encode_name(host));
        packet.extend_from_slice(&record(instance, TYPE_SRV, &srv));

        packet.extend_from_slice(&record(host, TYPE_A, &[192, 168, 1, 50]));
        packet
    }

    #[test]
    fn test_read_name_simple() {
        let data = encode_name("_scpi-raw._tcp.local");
        let (name, next) = read_name(&data, 0).unwrap();
        assert_eq!(name, "_scpi-raw._tcp.local");
        assert_eq!(next, data.len());
    }

    #[test]
    fn test_read_name_compression_pointer() {
        // "local" at offset 0, then "abc" + a pointer back to offset 0.
        let mut data = encode_name("local");
        let ptr_offset = data.len();
        data.push(3);
        data.extend_from_slice(b"abc");
        data.push(0xC0);
        data.push(0x00); // pointer to offset 0

        let (name, next) = read_name(&data, ptr_offset).unwrap();
        assert_eq!(name, "abc.local");
        assert_eq!(next, ptr_offset + 6);
    }

    #[test]
    fn test_read_name_pointer_loop() {
        // A pointer at offset 0 that points to itself must not spin forever.
        let data = [0xC0u8, 0x00];
        assert!(read_name(&data, 0).is_none());
    }

    #[test]
    fn test_read_name_truncation() {
        // Label claims 5 bytes but only 2 follow.
        assert!(read_name(&[5u8, b'a', b'b'], 0).is_none());
        // Empty input.
        assert!(read_name(&[], 0).is_none());
        // Pointer past the end of the packet.
        assert!(read_name(&[0xC0u8, 0xFF], 0).is_none());
    }

    #[test]
    fn test_normalise() {
        assert_eq!(normalise("Rigol.Local."), "rigol.local");
        assert_eq!(normalise("ABC"), "abc");
    }

    #[test]
    fn test_strip_service_suffix() {
        assert_eq!(
            strip_service_suffix("Rigol DS1054Z (abc)._scpi-raw._tcp.local"),
            "Rigol DS1054Z (abc)"
        );
        assert_eq!(strip_service_suffix("scope._lxi._tcp.local"), "scope");
        assert_eq!(strip_service_suffix("plain"), "plain");
    }

    #[test]
    fn test_strip_domain() {
        assert_eq!(strip_domain("rigol-abc.local"), "rigol-abc");
        assert_eq!(strip_domain("rigol-abc.local."), "rigol-abc");
        assert_eq!(strip_domain("rigol-abc"), "rigol-abc");
    }

    #[test]
    fn test_mdns_records_ingest() {
        let mut records = MdnsRecords::default();
        records.ingest(&sample_packet());

        assert_eq!(records.instances.len(), 1);
        assert!(
            records
                .srv
                .contains_key(&normalise("Rigol DS1054Z (abc)._scpi-raw._tcp.local"))
        );
        assert_eq!(
            records.addresses.get(&normalise("rigol-abc.local")),
            Some(&ip("192.168.1.50"))
        );
    }

    #[test]
    fn test_mdns_records_ingest_ignores_malformed() {
        let mut records = MdnsRecords::default();
        records.ingest(&[0u8; 4]); // too short for a header
        records.ingest(&[0u8; 12]); // header only, no records
        assert!(records.instances.is_empty());
    }

    #[test]
    fn test_mdns_records_into_devices() {
        let mut records = MdnsRecords::default();
        records.ingest(&sample_packet());
        let devices = records.into_devices();

        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device.address, ip("192.168.1.50"));
        assert_eq!(device.mdns_name.as_deref(), Some("Rigol DS1054Z (abc)"));
        assert_eq!(device.hostname.as_deref(), Some("rigol-abc"));
        assert_eq!(device.raw_port, Some(5025));
        assert_eq!(device.method, DiscoveryMethod::Mdns);
    }

    #[test]
    fn test_merge_prefers_richer_record() {
        let mut broadcast = Device::manual(ip("192.168.1.50"));
        broadcast.method = DiscoveryMethod::Vxi11Broadcast;
        broadcast.vxi11_port = Some(1024);

        let mut mdns = Device::manual(ip("192.168.1.50"));
        mdns.method = DiscoveryMethod::Mdns;
        mdns.mdns_name = Some("Rigol DS1054Z".to_string());
        mdns.hostname = Some("rigol-abc".to_string());
        mdns.raw_port = Some(5025);

        let merged = merge_devices(vec![broadcast], vec![mdns]);
        assert_eq!(merged.len(), 1);
        let device = &merged[0];
        // The friendly name comes from mDNS...
        assert_eq!(device.mdns_name.as_deref(), Some("Rigol DS1054Z"));
        assert_eq!(device.hostname.as_deref(), Some("rigol-abc"));
        // ...while the VXI-11 port is filled in from the broadcast record.
        assert_eq!(device.vxi11_port, Some(1024));
        assert_eq!(device.raw_port, Some(5025));
    }

    #[test]
    fn test_merge_keeps_distinct_ips() {
        let a = Device::manual(ip("192.168.1.50"));
        let b = Device::manual(ip("192.168.1.51"));
        let merged = merge_devices(vec![a], vec![b]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_discover_returns_within_window() {
        let start = Instant::now();
        // The result depends on the network; the contract under test is that
        // discovery is bounded by the timeout and never hangs.
        let _ = discover(Duration::from_millis(300), true);
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn test_discover_vxi11_returns_within_window() {
        let start = Instant::now();
        let _ = discover_vxi11(Duration::from_millis(200));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn test_discover_mdns_returns_within_window() {
        let start = Instant::now();
        let _ = discover_mdns(Duration::from_millis(200));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
