//! The device list a discovery publishes, and the stream its readers follow.

use crate::renderer::RendererDevice;

/// A discovery's device list, as one owner: the current snapshot and the
/// [`tokio::sync::watch`] sender that carries it to whoever is following. Every
/// discovery holds one — the three protocol browses ([`crate::cast`],
/// [`crate::dlna`], [`crate::airplay`]) and the reported path — so the picker
/// reads and follows them all the same way.
///
/// The watch channel holds the list itself, so publishing and reading cannot
/// disagree about what the current snapshot is.
///
/// Cloneable, because the protocol browses maintain their keyed device tables on
/// their own threads: a browse thread holds a clone and publishes into the same
/// list its discovery hands out receivers for.
#[derive(Clone)]
pub(crate) struct PublishedDevices {
    values: tokio::sync::watch::Sender<Vec<RendererDevice>>,
}

impl PublishedDevices {
    /// A list that starts out empty — a discovery has found nothing until it
    /// browses.
    pub(crate) fn new() -> Self {
        let (values, _) = tokio::sync::watch::channel(Vec::new());
        Self { values }
    }

    /// Follow the list. The current snapshot is available immediately on the
    /// returned receiver, which is then notified on each later publish.
    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<Vec<RendererDevice>> {
        self.values.subscribe()
    }

    /// The current snapshot, for a caller answering a one-shot request rather
    /// than following the stream.
    pub(crate) fn current(&self) -> Vec<RendererDevice> {
        self.values.borrow().clone()
    }

    /// Replace the list with a freshly computed snapshot. The whole list is
    /// published at once — a discovery keeps its own keyed table of what it has
    /// resolved and snapshots that table, since collapsing several services into
    /// one device is protocol-specific and happens before the list is published.
    pub(crate) fn publish(&self, devices: Vec<RendererDevice>) {
        self.values.send_replace(devices);
    }

    /// Drop back to an empty list at the start of a fresh browse, so a stale
    /// snapshot from a previous one isn't shown before the first result.
    pub(crate) fn clear(&self) {
        self.publish(Vec::new());
    }
}

impl Default for PublishedDevices {
    fn default() -> Self {
        Self::new()
    }
}
