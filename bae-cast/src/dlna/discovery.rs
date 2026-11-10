//! SSDP discovery of UPnP MediaRenderers on the local network.
//!
//! `DlnaDiscovery` mirrors the Cast discovery's shape — a live device list in a
//! `PublishedDevices`, browsing only while something lists receivers. The mechanics
//! differ: instead of an mDNS daemon, a search thread sends SSDP `M-SEARCH`
//! datagrams for `MediaRenderer:1`, then fetches and parses each responder's
//! device-description XML to learn its friendly name and the AVTransport /
//! RenderingControl control URLs the SOAP layer drives.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use crate::renderer::protocol_discovery::{Browse, ProtocolDiscovery};
use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::{RendererConnection, RendererDevice};
use bae_diagnostics::{Diagnostics, ReportedThread};
use bae_util::error_text::CauseChain;
use bae_util::task_failure::caught_sync;

/// The SSDP multicast endpoint every `M-SEARCH` is sent to.
const SSDP_MULTICAST: &str = "239.255.255.250:1900";
/// The device type searched for: a UPnP A/V MediaRenderer.
const MEDIA_RENDERER: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
/// How often the search is re-multicast so devices that missed or dropped the
/// first datagram still answer.
const RESEARCH_INTERVAL: Duration = Duration::from_secs(5);
/// Timeout for fetching a responder's device-description document.
const DESCRIPTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Browses for MediaRenderers over SSDP and publishes the current list as
/// [`RendererDevice`]s so the picker shows one merged list. Start and stop with
/// the picker's visibility; a stopped discovery holds no socket and no search
/// thread.
pub(crate) type DlnaDiscovery = ProtocolDiscovery<DlnaSearch>;

/// A live SSDP search: its stop flag, where a datagram wakes its reader from
/// a blocking read, and the thread draining responses.
pub(crate) struct DlnaSearch {
    stop: Arc<AtomicBool>,
    wake: SocketAddr,
    reader: ReportedThread,
}

impl Browse for DlnaSearch {
    fn start(devices: &PublishedDevices, diagnostics: &Diagnostics) -> Option<Self> {
        let (socket, wake) = match open_search_socket() {
            Ok(opened) => opened,
            Err(e) => {
                warn!("dlna discovery: failed to open SSDP socket: {e}");
                return None;
            }
        };
        devices.clear();
        let reader_devices = devices.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = stop.clone();
        let reader_diagnostics = diagnostics.clone();
        // A reader that panicked can no longer keep the list current, so the
        // list is emptied rather than left showing receivers it can't confirm.
        let panicked_devices = devices.clone();
        let reader = match diagnostics.spawn_thread_reported(
            "dlna search reader",
            move || run_search(socket, reader_stop, reader_devices, reader_diagnostics),
            move |_| panicked_devices.clear(),
        ) {
            Ok(reader) => reader,
            Err(error) => {
                warn!("dlna discovery: failed to start the search reader thread: {error}");
                return None;
            }
        };
        Some(Self { stop, wake, reader })
    }

    /// Set the stop flag, then wake the reader's read with a datagram so it
    /// sees the flag. A wake that can't be sent leaves the read to end at the
    /// next re-search.
    fn stop(self) {
        self.stop.store(true, Ordering::Release);
        if let Err(error) = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .and_then(|socket| socket.send_to(&[], self.wake))
        {
            warn!("dlna discovery: couldn't wake the search reader to stop it: {error}");
        }
        self.reader.join();
    }
}

/// Bind a UDP socket for the search, and the loopback address a datagram
/// wakes its reader at.
fn open_search_socket() -> std::io::Result<(UdpSocket, SocketAddr)> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    let wake = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), socket.local_addr()?.port());
    Ok((socket, wake))
}

/// The `M-SEARCH` datagram: an SSDP discover for MediaRenderers, `MX=2` seconds
/// of spread so many devices don't answer in the same instant. CRLF line endings
/// and the trailing blank line are required by the protocol.
fn msearch_datagram() -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: {SSDP_MULTICAST}\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: {MEDIA_RENDERER}\r\n\
         USER-AGENT: bae UPnP/1.0\r\n\
         \r\n"
    )
}

/// The search-thread body: multicast the search, then drain unicast answers,
/// fetching and parsing each new responder's description into a device. Re-sends
/// the search every [`RESEARCH_INTERVAL`] and publishes a snapshot whenever the
/// set changes. Each read blocks until a datagram arrives or the next re-search
/// is due; [`DlnaSearch::stop`] sets the stop flag and sends a datagram, and
/// the loop returns. A response whose handling panics is reported and skipped;
/// the search reads on.
fn run_search(
    socket: UdpSocket,
    stop: Arc<AtomicBool>,
    devices: PublishedDevices,
    diagnostics: Diagnostics,
) {
    let client = match reqwest::blocking::Client::builder().timeout(DESCRIPTION_TIMEOUT).build() {
        Ok(client) => client,
        Err(e) => {
            warn!("dlna discovery: failed to build HTTP client: {}", CauseChain(&e));
            return;
        }
    };

    let datagram = msearch_datagram();
    if let Err(e) = socket.send_to(datagram.as_bytes(), SSDP_MULTICAST) {
        warn!("dlna discovery: initial M-SEARCH send failed: {e}");
    }
    let mut last_search = Instant::now();

    // Every location fetched, so a renderer answering repeated searches is
    // described once; the device map is keyed by UDN so several NICs collapse.
    let mut fetched_locations: HashSet<String> = HashSet::new();
    let mut by_id: HashMap<String, RendererDevice> = HashMap::new();
    let mut buf = [0u8; 2048];

    loop {
        if last_search.elapsed() >= RESEARCH_INTERVAL {
            if let Err(e) = socket.send_to(datagram.as_bytes(), SSDP_MULTICAST) {
                debug!("dlna discovery: re-search send failed: {e}");
            }
            last_search = Instant::now();
        }
        // Read until the next re-search is due; a zero timeout is refused, so
        // at least a millisecond.
        let until_search =
            RESEARCH_INTERVAL.saturating_sub(last_search.elapsed()).max(Duration::from_millis(1));
        if let Err(e) = socket.set_read_timeout(Some(until_search)) {
            warn!("dlna discovery: can't time the read to the next re-search: {e}");
            return;
        }

        let received = socket.recv_from(&mut buf);
        if stop.load(Ordering::Acquire) {
            return;
        }
        let received = match received {
            Ok((len, _from)) => len,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                debug!("dlna discovery: recv failed: {e}");
                continue;
            }
        };

        let followed = caught_sync("follow a UPnP search response", || {
            let Some(location) = parse_ssdp_location(&String::from_utf8_lossy(&buf[..received]))
            else {
                return;
            };
            if !fetched_locations.insert(location.clone()) {
                return;
            }

            let found = fetch_devices(&client, &location);
            if found.is_empty() {
                debug!("dlna discovery: {location} describes no usable renderer");
                return;
            }
            for device in found {
                by_id.insert(device.id.clone(), device);
            }
            devices.publish(snapshot(&by_id));
        });
        if let Err(bug) = followed {
            diagnostics.panicked(bug);
        }
    }
}

/// Fetch and parse a responder's device-description document into the
/// renderers it describes; none when the fetch fails.
fn fetch_devices(client: &reqwest::blocking::Client, location: &str) -> Vec<RendererDevice> {
    let xml = match client.get(location).send().and_then(|r| r.text()) {
        Ok(xml) => xml,
        Err(e) => {
            debug!("dlna discovery: fetching {location} failed: {}", CauseChain(&e));
            return Vec::new();
        }
    };
    parse_device_description(&xml, location)
}

/// A stable, de-duplicated device list: one entry per UDN, in picker order.
fn snapshot(by_id: &HashMap<String, RendererDevice>) -> Vec<RendererDevice> {
    let mut devices: Vec<RendererDevice> = by_id.values().cloned().collect();
    crate::renderer::discovery::sort_for_picker(&mut devices);
    devices
}

/// The `LOCATION` header value from an SSDP search response — the URL of the
/// responder's device-description document. Header names are matched
/// case-insensitively (the protocol doesn't fix their case). `None` when absent.
pub(super) fn parse_ssdp_location(response: &str) -> Option<String> {
    for line in response.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("location") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Parse a device-description document (fetched from `location`) into the
/// UPnP [`RendererDevice`]s it describes: the root device and each device
/// embedded in it — a multi-zone receiver's zones — that has an AVTransport
/// service, each from its own elements alone. A document that doesn't parse
/// describes none (logged). A device with no UDN (UPnP tells devices apart by
/// it, so without one the renderer has no identity that survives its address
/// changing; logged with its address) or no AVTransport service (it can't be
/// played to) is left out. Control URLs are resolved to absolute form against
/// `URLBase` when present, else against `location` — so a relative
/// `controlURL`, an absolute-path one, and one naming another host/port all
/// land as a full URL.
pub(super) fn parse_device_description(xml: &str, location: &str) -> Vec<RendererDevice> {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(doc) => doc,
        Err(error) => {
            debug!("dlna discovery: the description at {location} doesn't parse: {error}");
            return Vec::new();
        }
    };

    let base = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "URLBase")
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(location);

    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "device")
        .filter_map(|device| parse_device(&device, base, location))
        .collect()
}

/// One `device` element as a renderer, read from its own child elements and
/// services, never those of the devices embedded in it.
fn parse_device(device: &roxmltree::Node, base: &str, location: &str) -> Option<RendererDevice> {
    let mut av_transport_url = None;
    let mut rendering_control_url = None;
    let services = child_element(device, "serviceList").into_iter().flat_map(|list| {
        list.children().filter(|n| n.is_element() && n.tag_name().name() == "service")
    });
    for service in services {
        let service_type = child_text(&service, "serviceType").unwrap_or_default();
        let Some(control_url) = child_text(&service, "controlURL") else {
            continue;
        };
        let resolved = resolve_url(base, control_url.trim());
        if service_type.contains("AVTransport") {
            av_transport_url = av_transport_url.or(resolved);
        } else if service_type.contains("RenderingControl") {
            rendering_control_url = rendering_control_url.or(resolved);
        }
    }
    let av_transport_url = av_transport_url?;

    let Some(id) =
        child_text(device, "UDN").map(str::trim).filter(|udn| !udn.is_empty()).map(str::to_string)
    else {
        warn!(
            "dlna discovery: a renderer at {location} states no UDN, which UPnP \
             needs to tell devices apart; leaving it out of the picker"
        );
        return None;
    };

    let name = child_text(device, "friendlyName")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name(location));

    Some(RendererDevice {
        id,
        name,
        connection: RendererConnection::Dlna { av_transport_url, rendering_control_url },
    })
}

/// A display name for a renderer that advertises no `friendlyName`: its host, so
/// the picker shows something addressable rather than a blank row.
fn fallback_name(location: &str) -> String {
    reqwest::Url::parse(location)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "UPnP Renderer".to_string())
}

/// The child element of `node` with local name `name`.
fn child_element<'a, 'input>(
    node: &roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|n| n.is_element() && n.tag_name().name() == name)
}

/// The text of the child element of `node` with local name `name`.
fn child_text<'a>(node: &roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    child_element(node, name).and_then(|n| n.text())
}

/// Resolve `target` against `base`, handling a relative path, an absolute path,
/// and a full URL on another host/port uniformly. `None` when neither parses.
fn resolve_url(base: &str, target: &str) -> Option<String> {
    reqwest::Url::parse(base).ok()?.join(target).ok().map(|url| url.to_string())
}
