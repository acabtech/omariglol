//! Network configuration detection (discovery subsystem).
//!
//! Discovery needs to know *where* to look: the local subnet, its prefix
//! length, the default gateway, whether the link is link-local, and which VLAN
//! the interface belongs to. This module answers those questions using only the
//! standard library and the Linux `/proc` filesystem — no extra dependencies.
//!
//! ## How it works
//!
//! 1. The primary local address is found with the UDP-connect trick: connecting
//!    a datagram socket to a routable address makes the kernel pick the
//!    outbound interface, and `local_addr()` reports the address it chose. No
//!    packet is sent.
//! 2. `/proc/net/route` is parsed for the directly-connected subnet route whose
//!    network contains that address; its mask gives the prefix length, and its
//!    interface name gives the VLAN.
//! 3. The default route (destination `0.0.0.0`, mask `0.0.0.0`) gives the
//!    gateway.
//!
//! On non-Linux targets the route table is unavailable, so only the primary
//! address is reported and the subnet is assumed to be a `/24`.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// A routable address used only to make the kernel choose an interface.
///
/// The UDP-connect trick never sends a packet, so this need not be reachable.
const ROUTE_PROBE: &str = "1.1.1.1:53";

/// The link-local prefix, `169.254.0.0/16`.
pub const LINK_LOCAL_PREFIX: Ipv4Addr = Ipv4Addr::new(169, 254, 0, 0);

/// A network interface and the subnet it is on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InterfaceInfo {
    /// The interface name, e.g. `eth0` or `eth0.100`.
    pub name: String,
    /// The interface's own IPv4 address.
    pub address: Ipv4Addr,
    /// The subnet mask.
    pub netmask: Ipv4Addr,
    /// The prefix length, 0–32.
    pub prefix_len: u8,
    /// The network address (`address & netmask`).
    pub network: Ipv4Addr,
    /// The directed broadcast address.
    pub broadcast: Ipv4Addr,
    /// The VLAN id, when the interface name or `/proc/net/vlan` reveals one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vlan: Option<u16>,
    /// True when the address is in the link-local range.
    pub is_link_local: bool,
}

impl InterfaceInfo {
    /// Build an interface description from an address and mask.
    pub fn new(name: impl Into<String>, address: Ipv4Addr, netmask: Ipv4Addr) -> Self {
        let prefix_len = mask_to_prefix(netmask);
        let network = Ipv4Addr::from(u32::from(address) & u32::from(netmask));
        let broadcast = Ipv4Addr::from(u32::from(network) | !u32::from(netmask));
        let name = name.into();
        InterfaceInfo {
            vlan: vlan_from_name(&name),
            name,
            address,
            netmask,
            prefix_len,
            network,
            broadcast,
            is_link_local: is_link_local(address),
        }
    }

    /// The number of usable host addresses on this subnet.
    ///
    /// Excludes the network and broadcast addresses. A `/31` or `/32` has no
    /// usable hosts in the conventional sense, so it reports zero.
    pub fn usable_host_count(&self) -> u64 {
        if self.prefix_len >= 31 {
            return 0;
        }
        (1u64 << (32 - self.prefix_len)) - 2
    }

    /// Every usable host address on this subnet, excluding the interface's own.
    ///
    /// Bounded by `limit` so a `/8` cannot exhaust memory; the caller is
    /// expected to pass a sane cap (a `/24` yields 253 addresses).
    pub fn candidate_hosts(&self, limit: usize) -> Vec<Ipv4Addr> {
        let mut hosts = Vec::new();
        if self.prefix_len >= 31 {
            return hosts;
        }
        let network = u32::from(self.network);
        let broadcast = u32::from(self.broadcast);
        let own = u32::from(self.address);
        let mut current = network.saturating_add(1);
        while current < broadcast && hosts.len() < limit {
            if current != own {
                hosts.push(Ipv4Addr::from(current));
            }
            current += 1;
        }
        hosts
    }
}

/// The detected network configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NetworkConfig {
    /// The interface carrying the default route, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<InterfaceInfo>,
    /// Every directly-connected subnet found in the route table.
    #[serde(default)]
    pub interfaces: Vec<InterfaceInfo>,
    /// The default gateway, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<Ipv4Addr>,
}

impl NetworkConfig {
    /// The primary subnet's candidate hosts, or an empty list when offline.
    pub fn candidate_hosts(&self, limit: usize) -> Vec<Ipv4Addr> {
        self.primary
            .as_ref()
            .map(|interface| interface.candidate_hosts(limit))
            .unwrap_or_default()
    }

    /// True when a usable primary interface was detected.
    pub fn is_online(&self) -> bool {
        self.primary.is_some()
    }
}

/// Count the set bits in a netmask, i.e. its prefix length.
pub fn mask_to_prefix(mask: Ipv4Addr) -> u8 {
    u32::from(mask).count_ones() as u8
}

/// True when an address is in the link-local range.
pub fn is_link_local(address: Ipv4Addr) -> bool {
    let value = u32::from(address);
    let prefix = u32::from(LINK_LOCAL_PREFIX);
    (value & 0xFFFF_0000) == prefix
}

/// Infer a VLAN id from an interface name such as `eth0.100`.
fn vlan_from_name(name: &str) -> Option<u16> {
    name.rsplit_once('.')
        .and_then(|(_, suffix)| suffix.parse::<u16>().ok())
}

/// A parsed `/proc/net/route` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteEntry {
    iface: String,
    destination: Ipv4Addr,
    gateway: Ipv4Addr,
    mask: Ipv4Addr,
}

/// Parse the Linux route table.
///
/// The hex fields are little-endian, so `0002A8C0` is `192.168.2.0`.
fn parse_route_table(text: &str) -> Vec<RouteEntry> {
    let mut entries = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 8 {
            continue;
        }
        let (Some(destination), Some(gateway), Some(mask)) = (
            parse_hex_ip(fields[1]),
            parse_hex_ip(fields[2]),
            parse_hex_ip(fields[7]),
        ) else {
            continue;
        };
        entries.push(RouteEntry {
            iface: fields[0].to_string(),
            destination,
            gateway,
            mask,
        });
    }
    entries
}

/// Parse a little-endian hex IPv4 field from `/proc/net/route`.
fn parse_hex_ip(field: &str) -> Option<Ipv4Addr> {
    let value = u32::from_str_radix(field, 16).ok()?;
    Some(Ipv4Addr::from(value.to_le_bytes()))
}

/// Find the primary local IPv4 address using the UDP-connect trick.
fn primary_local_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Connecting a UDP socket sends nothing; it only selects a route.
    socket.connect(ROUTE_PROBE).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) => Some(ip),
        IpAddr::V6(_) => None,
    }
}

/// Detect the current network configuration.
///
/// Never fails: an offline host yields an empty configuration, which callers
/// treat as "nothing to scan".
pub fn detect() -> NetworkConfig {
    let Some(local_ip) = primary_local_ip() else {
        return NetworkConfig::default();
    };

    let route_text = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
    let routes = parse_route_table(&route_text);

    // The default gateway is the route with a zero destination and mask.
    let gateway = routes
        .iter()
        .find(|route| route.destination.is_unspecified() && route.mask.is_unspecified())
        .map(|route| route.gateway)
        .filter(|gateway| !gateway.is_unspecified());

    // Every directly-connected subnet (non-zero mask).
    let mut interfaces = Vec::new();
    for route in &routes {
        if route.mask.is_unspecified() {
            continue;
        }
        // The route's destination is the network address; recover a host
        // address by OR-ing in the local address when it belongs to this subnet.
        let network = u32::from(route.destination);
        let mask = u32::from(route.mask);
        let host = if (u32::from(local_ip) & mask) == network {
            local_ip
        } else {
            // A subnet we are not on: report the network address as the host.
            route.destination
        };
        let interface = InterfaceInfo::new(route.iface.clone(), host, route.mask);
        if !interfaces.contains(&interface) {
            interfaces.push(interface);
        }
    }

    // The primary interface is the one whose subnet contains the local address.
    let primary = interfaces
        .iter()
        .find(|interface| {
            (u32::from(local_ip) & u32::from(interface.netmask)) == u32::from(interface.network)
        })
        .cloned()
        .or_else(|| {
            // No route table (non-Linux): assume a /24 around the local address.
            Some(InterfaceInfo::new(
                "unknown",
                local_ip,
                Ipv4Addr::new(255, 255, 255, 0),
            ))
        });

    NetworkConfig {
        primary,
        interfaces,
        gateway,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_to_prefix() {
        assert_eq!(mask_to_prefix(Ipv4Addr::new(255, 255, 255, 0)), 24);
        assert_eq!(mask_to_prefix(Ipv4Addr::new(255, 255, 0, 0)), 16);
        assert_eq!(mask_to_prefix(Ipv4Addr::new(255, 255, 255, 255)), 32);
        assert_eq!(mask_to_prefix(Ipv4Addr::new(0, 0, 0, 0)), 0);
    }

    #[test]
    fn test_is_link_local() {
        assert!(is_link_local(Ipv4Addr::new(169, 254, 1, 1)));
        assert!(!is_link_local(Ipv4Addr::new(192, 168, 1, 1)));
    }

    #[test]
    fn test_interface_info_derives_network_and_broadcast() {
        let interface = InterfaceInfo::new(
            "eth0",
            Ipv4Addr::new(192, 168, 1, 50),
            Ipv4Addr::new(255, 255, 255, 0),
        );
        assert_eq!(interface.prefix_len, 24);
        assert_eq!(interface.network, Ipv4Addr::new(192, 168, 1, 0));
        assert_eq!(interface.broadcast, Ipv4Addr::new(192, 168, 1, 255));
        assert!(!interface.is_link_local);
        assert_eq!(interface.vlan, None);
    }

    #[test]
    fn test_interface_info_vlan_from_name() {
        let interface = InterfaceInfo::new(
            "eth0.100",
            Ipv4Addr::new(10, 0, 0, 5),
            Ipv4Addr::new(255, 255, 255, 0),
        );
        assert_eq!(interface.vlan, Some(100));
    }

    #[test]
    fn test_usable_host_count() {
        let interface = InterfaceInfo::new(
            "eth0",
            Ipv4Addr::new(192, 168, 1, 50),
            Ipv4Addr::new(255, 255, 255, 0),
        );
        assert_eq!(interface.usable_host_count(), 254);
    }

    #[test]
    fn test_candidate_hosts_excludes_self_and_edges() {
        let interface = InterfaceInfo::new(
            "eth0",
            Ipv4Addr::new(192, 168, 1, 50),
            Ipv4Addr::new(255, 255, 255, 0),
        );
        let hosts = interface.candidate_hosts(1000);
        assert_eq!(hosts.len(), 253);
        assert!(!hosts.contains(&Ipv4Addr::new(192, 168, 1, 50)));
        assert!(!hosts.contains(&Ipv4Addr::new(192, 168, 1, 0)));
        assert!(!hosts.contains(&Ipv4Addr::new(192, 168, 1, 255)));
        assert!(hosts.contains(&Ipv4Addr::new(192, 168, 1, 1)));
    }

    #[test]
    fn test_candidate_hosts_respects_limit() {
        let interface = InterfaceInfo::new(
            "eth0",
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(255, 0, 0, 0),
        );
        assert_eq!(interface.candidate_hosts(10).len(), 10);
    }

    #[test]
    fn test_parse_route_table() {
        let text = "\
Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT
eth0\t00000000\t0102A8C0\t0003\t0\t0\t0\t00000000\t0\t0\t0
eth0\t0002A8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0
";
        let routes = parse_route_table(text);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].destination, Ipv4Addr::new(0, 0, 0, 0));
        assert_eq!(routes[0].gateway, Ipv4Addr::new(192, 168, 2, 1));
        assert_eq!(routes[1].destination, Ipv4Addr::new(192, 168, 2, 0));
        assert_eq!(routes[1].mask, Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(routes[1].iface, "eth0");
    }

    #[test]
    fn test_parse_route_table_skips_malformed() {
        let text = "Iface\tDestination\neth0\tshort\n";
        assert!(parse_route_table(text).is_empty());
    }

    #[test]
    fn test_detect_never_panics() {
        // The result depends on the host; the contract is that it returns.
        let config = detect();
        if let Some(primary) = &config.primary {
            assert!(primary.prefix_len <= 32);
        }
    }
}
