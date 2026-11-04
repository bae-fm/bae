//! Fixtures the discovery tests share: an IPv4 address and an mDNS TXT record.

use std::collections::HashMap;
use std::net::IpAddr;

pub(crate) fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::from([a, b, c, d])
}

pub(crate) fn txt(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}
