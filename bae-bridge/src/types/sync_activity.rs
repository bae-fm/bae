//! Everything sync is doing on this device, as the activity indicator and
//! its list show it.

/// Everything sync is doing on this device, for the activity indicator in
/// the title bar and the list it opens. Mirrors `bae_core::library::SyncActivity`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncActivity {
    /// Bytes are moving: the indicator spins. A pass alone doesn't count.
    pub moving: bool,
    /// Something needs the person: the indicator shows a warning, and the
    /// list leads with it.
    pub needs_attention: bool,
    /// The sync failure the status line shows, such as being offline.
    pub failure: Option<super::BridgeError>,
    /// The pending records that need the person.
    pub attention: super::BridgePendingRecords,
    /// The uploads this device abandoned, until the person dismisses each.
    pub abandoned: super::BridgeAbandonedAttachments,
    /// A sync pass is running.
    pub syncing: bool,
    pub uploads: Option<BridgeUploadsInFlight>,
    pub downloads: Option<BridgeDownloadsInFlight>,
}

impl BridgeSyncActivity {
    pub(crate) fn from_core(activity: bae_core::library::SyncActivity) -> Self {
        let moving = activity.moving();
        let needs_attention = activity.needs_attention();
        let bae_core::library::SyncActivity {
            failure,
            attention,
            abandoned,
            syncing,
            uploads,
            downloads,
        } = activity;
        Self {
            moving,
            needs_attention,
            failure: failure.map(super::BridgeError::from_core),
            attention: super::BridgePendingRecords::from_core(attention),
            abandoned: super::BridgeAbandonedAttachments::from_core(abandoned),
            syncing,
            uploads: uploads.map(BridgeUploadsInFlight::from_core),
            downloads: downloads.map(BridgeDownloadsInFlight::from_core),
        }
    }
}

/// The files this device is sending now, out of its whole upload queue.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadsInFlight {
    /// The list's line: the counts sending and waiting, a zero dropped.
    pub summary_parts: Vec<super::BridgeCountLabel>,
    /// Provider-confirmed bytes of the files sent in parts; zero of zero when
    /// every file goes in one request.
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl BridgeUploadsInFlight {
    fn from_core(uploads: bae_core::library::UploadsInFlight) -> Self {
        Self {
            summary_parts: uploads
                .summary_parts()
                .into_iter()
                .map(super::BridgeCountLabel::from_core)
                .collect(),
            bytes_done: uploads.bytes_done,
            bytes_total: uploads.bytes_total,
        }
    }
}

/// The releases downloading to keep offline.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDownloadsInFlight {
    /// The list's line: the counts downloading and waiting, a zero dropped.
    pub summary_parts: Vec<super::BridgeCountLabel>,
}

impl BridgeDownloadsInFlight {
    fn from_core(downloads: bae_core::library::DownloadsInFlight) -> Self {
        Self {
            summary_parts: downloads
                .summary_parts()
                .into_iter()
                .map(super::BridgeCountLabel::from_core)
                .collect(),
        }
    }
}
