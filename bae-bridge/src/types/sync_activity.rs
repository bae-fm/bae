//! Everything sync is doing on this device, as the activity indicator and
//! the Activity panel show it.

/// Everything sync is doing on this device, for the activity indicator in
/// the title bar and the Activity panel it opens. Mirrors
/// `bae_views::SyncActivity`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncActivity {
    /// Bytes are moving: the indicator spins. A pass alone doesn't count.
    pub moving: bool,
    /// A queue is held and no bytes move: the indicator shows a pause
    /// symbol, and that queue's line says what holds it.
    pub held: bool,
    /// Something needs the person: the indicator shows a warning, and the
    /// panel leads with it.
    pub needs_attention: bool,
    /// The sync failure the status line shows, such as being offline.
    pub failure: Option<super::BridgeError>,
    /// The pending records that need the person.
    pub attention: Vec<super::BridgePendingRecord>,
    /// coven's sync state is syncing: a pass is moving, or a file is being
    /// sent or downloaded. The panel's Syncing changes line shows while it is.
    pub syncing: bool,
    /// The Uploading line, while its batch holds a release.
    pub uploads: Option<BridgeUploadQueueActivity>,
    /// The Pinning line, while its batch holds a release.
    pub pins: Option<BridgeDownloadQueueActivity>,
    /// The Moving to This Mac line, while its batch holds a release; never on a
    /// phone, which makes nothing local.
    pub moving_to_this_device: Option<BridgeDownloadQueueActivity>,
}

impl BridgeSyncActivity {
    pub(crate) fn from_core(activity: bae_views::SyncActivity) -> Self {
        let needs_attention = activity.needs_attention();
        let held = activity.held();
        let bae_views::SyncActivity {
            failure,
            attention,
            syncing,
            moving,
            uploads,
            pins,
            moving_to_this_device,
        } = activity;
        Self {
            moving,
            held,
            needs_attention,
            failure: failure.map(super::BridgeError::from_core),
            attention: attention.into_iter().map(super::BridgePendingRecord::from_core).collect(),
            syncing,
            uploads: uploads.map(BridgeUploadQueueActivity::from_core),
            pins: pins.map(BridgeDownloadQueueActivity::from_core),
            moving_to_this_device: moving_to_this_device
                .map(BridgeDownloadQueueActivity::from_core),
        }
    }
}

/// The releases a transfer line's batch holds, and how many of them are
/// done: the line's count, "1 of 3 releases". Mirrors
/// `bae_views::TransferBatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTransferBatch {
    pub done: u32,
    pub total: u32,
}

mirror_struct! {
    BridgeTransferBatch = bae_views::TransferBatch,
    from_core: fn,
    fields: { done, total },
}

/// The Uploading line: the releases with files queued to upload since none
/// had any. Mirrors `bae_views::UploadQueueActivity`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadQueueActivity {
    pub batch: BridgeTransferBatch,
    /// What holds the upload queue back, shown in place of the count.
    pub wait: Option<super::BridgeUploadQueueWait>,
    /// The person paused the upload queue: the line ends in Resume, not Pause.
    pub paused: bool,
}

impl BridgeUploadQueueActivity {
    fn from_core(uploads: bae_views::UploadQueueActivity) -> Self {
        Self {
            batch: BridgeTransferBatch::from_core(uploads.batch),
            paused: uploads.paused(),
            wait: uploads.wait.map(super::BridgeUploadQueueWait::from_core),
        }
    }
}

/// The Pinning or the Moving to This Mac line: the releases queued as that kind
/// since none of it waited or downloaded. Mirrors
/// `bae_views::DownloadQueueActivity`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDownloadQueueActivity {
    pub batch: BridgeTransferBatch,
    /// What holds the download queue back, shown in place of the count.
    pub wait: Option<super::BridgeQueueWait>,
    /// The person paused the download queue: the line ends in Resume, not
    /// Pause.
    pub paused: bool,
}

impl BridgeDownloadQueueActivity {
    fn from_core(downloads: bae_views::DownloadQueueActivity) -> Self {
        Self {
            batch: BridgeTransferBatch::from_core(downloads.batch),
            paused: downloads.paused(),
            wait: downloads.wait.map(super::BridgeQueueWait::from_core),
        }
    }
}

/// What the person's retry came to: the first sync pass after the request.
/// Mirrors `bae_sync_status::SyncRetryOutcome`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSyncRetryOutcome {
    /// The pass reached its read end.
    Synced,
    /// The pass failed, or a failure only the person can clear holds every
    /// pass. The error renders through its own line.
    Failed { error: super::BridgeError },
    /// The storage provider asked every request to wait until `until`; the
    /// pass runs once it ends. The UI formats `until` for the locale into the
    /// line's `until` argument.
    CoolingDown { until: std::time::SystemTime },
    /// Sync isn't running, so no pass followed the request.
    NotSyncing,
}

impl BridgeSyncRetryOutcome {
    pub(crate) fn from_core(outcome: bae_sync_status::SyncRetryOutcome) -> Self {
        match outcome {
            bae_sync_status::SyncRetryOutcome::Synced => Self::Synced,
            bae_sync_status::SyncRetryOutcome::Failed(error) => {
                Self::Failed { error: super::BridgeError::from_core(error) }
            }
            bae_sync_status::SyncRetryOutcome::CoolingDown { until } => Self::CoolingDown { until },
            bae_sync_status::SyncRetryOutcome::NotSyncing => Self::NotSyncing,
        }
    }
}

/// The line for a retry that didn't sync and has no error of its own: the
/// provider's cooldown, or sync not running. `None` for a synced retry, which
/// shows nothing, and a failed one, whose error has its line.
#[uniffi::export]
pub fn bridge_sync_retry_outcome_key(outcome: &BridgeSyncRetryOutcome) -> Option<String> {
    match outcome {
        BridgeSyncRetryOutcome::Synced | BridgeSyncRetryOutcome::Failed { .. } => None,
        BridgeSyncRetryOutcome::CoolingDown { .. } => {
            Some("core.sync.retry.cooling_down".to_string())
        }
        BridgeSyncRetryOutcome::NotSyncing => Some("core.sync.retry.not_syncing".to_string()),
    }
}
