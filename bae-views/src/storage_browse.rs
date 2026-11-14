//! What the Storage Manager shows beside its list's windows, count and
//! positions: its selection and the transfer queues holding work.

/// The rows of the selected releases the Storage Manager list holds,
/// wherever in it they sit, and the storage actions every one of them
/// allows: a selected release the list no longer holds is absent, so the
/// inspector never shows, and the storage actions never act on, a release the
/// list doesn't.
#[derive(Debug, Clone)]
pub struct StorageSelection {
    /// The releases the selection was asked about, held or not: which
    /// selection this answers.
    pub asked: Vec<String>,
    pub selected: Vec<crate::album_detail::StorageRow>,
    pub selection_actions: Vec<bae_types::release_storage::ReleaseStorageAction>,
    /// Whether some selected release is mid-transition or its move failed:
    /// the selection offers only cancelling, which ends those.
    pub cancel_only: bool,
}

/// The transfer queues holding files waiting or on the move, each with what
/// holds it back: this device's uploads, its downloads (pins and
/// moves-to-this-device, which share one queue), and its exports and saves. A queue holding no work, or only failed
/// entries, is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageQueues {
    pub uploads: Option<UploadQueueRun>,
    pub downloads: Option<ReleaseQueueRun>,
    pub outputs: Option<ReleaseQueueRun>,
}

/// The upload queue while it holds files, and what holds it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadQueueRun {
    pub wait: Option<bae_records::UploadQueueWait>,
}

/// A queue that works through one release at a time, while it holds work:
/// what holds it back, and the release it runs, if any; how fast that goes,
/// or how far, arrives apart, keyed by the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseQueueRun {
    pub wait: Option<bae_transfers::QueueWait>,
    pub active: Option<String>,
}
