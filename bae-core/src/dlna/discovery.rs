//! SSDP discovery of UPnP MediaRenderers on the local network.
//!
//! [`DlnaDiscovery`] mirrors the Cast discovery's shape — a live device list
//! published over a [`tokio::sync::watch`] channel, browsing only while the
//! picker is open. The mechanics differ: instead of an mDNS daemon, a search
//! thread sends SSDP `M-SEARCH` datagrams for `MediaRenderer:1`, then fetches and
//! parses each responder's device-description XML to learn its friendly name and
//! the AVTransport / RenderingControl control URLs the SOAP layer drives.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tracing::{debug, warn};

/// The SSDP multicast endpoint every `M-SEARCH` is sent to.
const SSDP_MULTICAST: &str = "239.255.255.250:1900";
/// The device type searched for: a UPnP A/V MediaRenderer.
const MEDIA_RENDERER: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
/// How long a `recv` blocks before the loop re-checks the stop flag.
const RECV_TIMEOUT: Duration = Duration::from_millis(900);
/// How often the search is re-multicast so devices that missed or dropped the
/// first datagram still answer.
const RESEARCH_INTERVAL: Duration = Duration::from_secs(5);
/// Timeout for fetching a responder's device-description document.
const DESCRIPTION_TIMEOUT: Duration = Duration::from_secs(5);

/// A discovered MediaRenderer: enough to display it and to drive it over SOAP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlnaDevice {
    /// Stable device identifier (the device's UDN, `uuid:…`), used to route
    /// playback and to de-duplicate a device that answers several searches.
    pub id: String,
    /// Human-readable name to show in the picker (the device's `friendlyName`).
    pub name: String,
    /// Absolute control URL for the AVTransport service (load/play/seek/stop).
    pub av_transport_url: String,
    /// Absolute control URL for RenderingControl (volume), when the renderer
    /// advertises the service. `None` renderers simply don't take a volume set.
    pub rendering_control_url: Option<String>,
}

/// Browses for MediaRenderers over SSDP and publishes the current list. Start and
/// stop with the picker's visibility; a stopped discovery holds no socket and no
/// search thread.
pub struct DlnaDiscovery {
    devices_tx: tokio::sync::watch::Sender<Vec<DlnaDevice>>,
    devices_rx: tokio::sync::watch::Receiver<Vec<DlnaDevice>>,
    /// The running search: its stop flag and the thread draining responses.
    /// `None` while stopped.
    running: Option<Running>,
}

struct Running {
    stop: Arc<AtomicBool>,
    reader: JoinHandle<()>,
}

impl DlnaDiscovery {
    pub fn new() -> Self {
        let (devices_tx, devices_rx) = tokio::sync::watch::channel(Vec::new());
        Self {
            devices_tx,
            devices_rx,
            running: None,
        }
    }

    /// Subscribe to the live device list. The current snapshot is available
    /// immediately on the returned receiver.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Vec<DlnaDevice>> {
        self.devices_rx.clone()
    }

    /// The current device list snapshot.
    pub fn devices(&self) -> Vec<DlnaDevice> {
        self.devices_rx.borrow().clone()
    }

    /// Begin searching. Idempotent: a second call while already searching is a
    /// no-op. Publishes list updates as renderers answer.
    pub fn start(&mut self) {
        if self.running.is_some() {
            return;
        }
        let socket = match open_search_socket() {
            Ok(socket) => socket,
            Err(e) => {
                warn!("dlna discovery: failed to open SSDP socket: {e}");
                return;
            }
        };
        // Reset to an empty list at the start of a fresh search so a stale
        // snapshot from a previous session isn't shown before the first answer.
        self.devices_tx.send_replace(Vec::new());
        let devices_tx = self.devices_tx.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = stop.clone();
        let reader = std::thread::spawn(move || run_search(socket, reader_stop, devices_tx));
        self.running = Some(Running { stop, reader });
    }

    /// Stop searching and release the socket. The last published device list is
    /// kept on the watch channel.
    pub fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.stop.store(true, Ordering::Relaxed);
        if running.reader.join().is_err() {
            warn!("dlna discovery: search reader thread panicked");
        }
    }
}

impl Default for DlnaDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DlnaDiscovery {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Bind a UDP socket for the search and set the recv timeout that lets the loop
/// re-check its stop flag between datagrams.
fn open_search_socket() -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_read_timeout(Some(RECV_TIMEOUT))?;
    Ok(socket)
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
/// set changes. Returns when the stop flag is set.
fn run_search(
    socket: UdpSocket,
    stop: Arc<AtomicBool>,
    devices_tx: tokio::sync::watch::Sender<Vec<DlnaDevice>>,
) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(DESCRIPTION_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            warn!("dlna discovery: failed to build HTTP client: {e}");
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
    let mut by_id: HashMap<String, DlnaDevice> = HashMap::new();
    let mut buf = [0u8; 2048];

    while !stop.load(Ordering::Relaxed) {
        if last_search.elapsed() >= RESEARCH_INTERVAL {
            if let Err(e) = socket.send_to(datagram.as_bytes(), SSDP_MULTICAST) {
                debug!("dlna discovery: re-search send failed: {e}");
            }
            last_search = Instant::now();
        }

        let received = match socket.recv_from(&mut buf) {
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

        let Some(location) = parse_ssdp_location(&String::from_utf8_lossy(&buf[..received])) else {
            continue;
        };
        if !fetched_locations.insert(location.clone()) {
            continue;
        }

        match fetch_device(&client, &location) {
            Some(device) => {
                by_id.insert(device.id.clone(), device);
                if devices_tx.send(snapshot(&by_id)).is_err() {
                    debug!("dlna discovery: no watchers for the device list");
                }
            }
            None => debug!("dlna discovery: {location} is not a usable renderer"),
        }
    }
}

/// Fetch and parse a responder's device-description document into a device, or
/// `None` when the fetch fails or the description isn't a usable renderer.
fn fetch_device(client: &reqwest::blocking::Client, location: &str) -> Option<DlnaDevice> {
    let xml = match client.get(location).send().and_then(|r| r.text()) {
        Ok(xml) => xml,
        Err(e) => {
            debug!("dlna discovery: fetching {location} failed: {e}");
            return None;
        }
    };
    parse_device_description(&xml, location)
}

/// A stable, de-duplicated device list: one entry per UDN, sorted by name for a
/// stable UI.
fn snapshot(by_id: &HashMap<String, DlnaDevice>) -> Vec<DlnaDevice> {
    let mut devices: Vec<DlnaDevice> = by_id.values().cloned().collect();
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
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

/// Parse a device-description document (fetched from `location`) into a
/// [`DlnaDevice`]. Returns `None` when the XML doesn't parse or the device has no
/// AVTransport service (it can't be played to). Control URLs are resolved to
/// absolute form against `URLBase` when present, else against `location` — so a
/// relative `controlURL`, an absolute-path one, and one naming another host/port
/// all land as a full URL.
pub(super) fn parse_device_description(xml: &str, location: &str) -> Option<DlnaDevice> {
    let doc = roxmltree::Document::parse(xml).ok()?;

    let base = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "URLBase")
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(location);

    let id = local_text(&doc, "UDN")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| location.to_string());

    let name = local_text(&doc, "friendlyName")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name(location));

    let mut av_transport_url = None;
    let mut rendering_control_url = None;
    for service in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "service")
    {
        let service_type = child_local_text(&service, "serviceType").unwrap_or_default();
        let Some(control_url) = child_local_text(&service, "controlURL") else {
            continue;
        };
        let resolved = resolve_url(base, control_url.trim());
        if service_type.contains("AVTransport") {
            av_transport_url = av_transport_url.or(resolved);
        } else if service_type.contains("RenderingControl") {
            rendering_control_url = rendering_control_url.or(resolved);
        }
    }

    Some(DlnaDevice {
        id,
        name,
        av_transport_url: av_transport_url?,
        rendering_control_url,
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

/// The text of the first element with local name `name` anywhere in the document.
fn local_text<'a>(doc: &'a roxmltree::Document, name: &str) -> Option<&'a str> {
    doc.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
}

/// The text of the first descendant of `node` with local name `name`.
fn child_local_text(node: &roxmltree::Node, name: &str) -> Option<String> {
    node.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
        .map(str::to_string)
}

/// Resolve `target` against `base`, handling a relative path, an absolute path,
/// and a full URL on another host/port uniformly. `None` when neither parses.
fn resolve_url(base: &str, target: &str) -> Option<String> {
    reqwest::Url::parse(base)
        .ok()?
        .join(target)
        .ok()
        .map(|url| url.to_string())
}
