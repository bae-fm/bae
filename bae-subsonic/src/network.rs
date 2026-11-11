//! The networks this machine is on, read from the system, and notice of their
//! addresses changing.

use std::net::Ipv4Addr;

use tokio_stream::StreamExt;

/// One IPv4 address the machine holds on a network interface, which the
/// Subsonic server can be reached at.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkAddress {
    /// The interface's BSD name, such as `en0`.
    pub interface: String,
    pub address: Ipv4Addr,
}

/// Reads the machine's addresses now. A failure to read them is the system's
/// word, as text.
pub type NetworkLister = dyn Fn() -> Result<Vec<NetworkAddress>, String> + Send + Sync;

/// The addresses a Subsonic app on another device could reach this machine
/// at, by interface name and address: every IPv4 address except the loopback
/// one and the link-local ones a network interface gives itself when it found
/// no network.
pub fn machine_addresses() -> Result<Vec<NetworkAddress>, String> {
    let mut addresses: Vec<NetworkAddress> = if_addrs::get_if_addrs()
        .map_err(|error| format!("couldn't read this machine's network addresses: {error}"))?
        .into_iter()
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(v4)
                if !interface.is_loopback()
                    && !v4.ip.is_link_local()
                    && !v4.ip.is_unspecified() =>
            {
                Some(NetworkAddress { interface: interface.name, address: v4.ip })
            }
            _ => None,
        })
        .collect();
    addresses.sort();
    Ok(addresses)
}

/// Notice that an address came, changed or went on some interface. It reports
/// that something changed, not what: the addresses are read again with
/// [`machine_addresses`].
pub struct AddressChanges(if_watch::tokio::IfWatcher);

impl AddressChanges {
    /// Start watching. Must run inside a tokio runtime.
    pub fn start() -> Result<Self, String> {
        if_watch::tokio::IfWatcher::new()
            .map(Self)
            .map_err(|error| format!("couldn't watch this machine's network addresses: {error}"))
    }

    /// Wait until an address changes.
    pub async fn changed(&mut self) -> Result<(), String> {
        match self.0.next().await {
            Some(Ok(_)) => Ok(()),
            Some(Err(error)) => {
                Err(format!("watching this machine's network addresses failed: {error}"))
            }
            None => Err("watching this machine's network addresses ended".to_string()),
        }
    }
}
