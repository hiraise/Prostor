//! Who is on the other end of a connection: the client's IP address, also behind reverse proxies
//! the administrator trusts (`TRACKFLOW_TRUSTED_PROXIES`). Used for sign-in throttling and the
//! session list.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use axum::http::request::Parts;

/// An address range in CIDR notation (`10.0.0.0/8`, `fd00::/8`) or a single address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpRange {
    network: IpAddr,
    prefix: u8,
}

impl IpRange {
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, canonical(ip)) {
            (IpAddr::V4(network), IpAddr::V4(ip)) => {
                prefix_matches(&network.octets(), &ip.octets(), self.prefix)
            }
            (IpAddr::V6(network), IpAddr::V6(ip)) => {
                prefix_matches(&network.octets(), &ip.octets(), self.prefix)
            }
            _ => false,
        }
    }
}

impl FromStr for IpRange {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (text, None),
        };
        let network = address
            .parse::<IpAddr>()
            .map(canonical)
            .map_err(|_| format!("{text:?} is not an IP address or range"))?;
        let max = if network.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            None => max,
            Some(prefix) => prefix
                .parse::<u8>()
                .ok()
                .filter(|prefix| *prefix <= max)
                .ok_or_else(|| format!("{text:?} has an invalid prefix length"))?,
        };
        Ok(Self { network, prefix })
    }
}

impl fmt::Display for IpRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix)
    }
}

fn prefix_matches(network: &[u8], ip: &[u8], prefix: u8) -> bool {
    let full = usize::from(prefix / 8);
    let rest = prefix % 8;
    if network[..full] != ip[..full] {
        return false;
    }
    if rest == 0 {
        return true;
    }
    let mask = 0xffu8 << (8 - rest);
    (network[full] & mask) == (ip[full] & mask)
}

/// IPv4 addresses written as IPv6 (`::ffff:10.0.0.1`) count as IPv4.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

/// The client's address: the peer of the connection, or — when the peer is a trusted proxy — the
/// nearest address in `X-Forwarded-For` that is not one of the trusted proxies.
pub fn client_ip(parts: &Parts, trusted: &[IpRange]) -> Option<IpAddr> {
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| canonical(address.ip()))?;
    Some(resolve(peer, &parts.headers, trusted))
}

fn resolve(peer: IpAddr, headers: &HeaderMap, trusted: &[IpRange]) -> IpAddr {
    let is_trusted = |ip: IpAddr| trusted.iter().any(|range| range.contains(ip));
    if !is_trusted(peer) {
        return peer;
    }
    // Proxies append the address they saw; read from the nearest hop backwards.
    let forwarded: Vec<IpAddr> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|hop| hop.trim().parse::<IpAddr>().ok().map(canonical))
        .collect();
    forwarded
        .iter()
        .rev()
        .copied()
        .find(|ip| !is_trusted(*ip))
        .or_else(|| forwarded.first().copied())
        .unwrap_or(peer)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn ranges() {
        let private: IpRange = "10.0.0.0/8".parse().unwrap();
        assert!(private.contains(ip("10.20.30.40")));
        assert!(!private.contains(ip("11.0.0.1")));
        assert!(private.contains(ip("::ffff:10.1.2.3")));
        let docker: IpRange = "172.16.0.0/12".parse().unwrap();
        assert!(docker.contains(ip("172.31.255.255")));
        assert!(!docker.contains(ip("172.32.0.1")));
        let single: IpRange = "127.0.0.1".parse().unwrap();
        assert!(single.contains(ip("127.0.0.1")) && !single.contains(ip("127.0.0.2")));
        let v6: IpRange = "fd00::/8".parse().unwrap();
        assert!(v6.contains(ip("fd12::1")) && !v6.contains(ip("fe80::1")));
        assert!(!v6.contains(ip("10.0.0.1")));
        for bad in ["10.0.0.0/33", "nonsense", "::/129", "1.2.3.4/x"] {
            assert!(bad.parse::<IpRange>().is_err(), "{bad}");
        }
    }

    #[test]
    fn forwarded_addresses_count_only_behind_trusted_proxies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.9, 198.51.100.7, 10.0.0.2"),
        );
        let proxies = ["10.0.0.0/8".parse().unwrap()];
        // A direct client cannot claim another address.
        assert_eq!(
            resolve(ip("192.0.2.1"), &headers, &proxies),
            ip("192.0.2.1")
        );
        // Behind the proxy: the nearest untrusted hop, not the first (which the client wrote).
        assert_eq!(
            resolve(ip("10.0.0.1"), &headers, &proxies),
            ip("198.51.100.7")
        );
        // A proxy without the header: its own address.
        assert_eq!(
            resolve(ip("10.0.0.1"), &HeaderMap::new(), &proxies),
            ip("10.0.0.1")
        );
        // Nobody trusted: the header is ignored.
        assert_eq!(resolve(ip("10.0.0.1"), &headers, &[]), ip("10.0.0.1"));
    }
}
