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
    /// A sync pass is running.
    pub syncing: bool,
    pub uploads: Option<BridgeUploadsInFlight>,
    pub downloads: Option<BridgeDownloadsInFlight>,
    /// The list offers Retry Now: a retry would run a pass, and a failure
    /// that pass can clear holds work back.
    pub retry_offered: bool,
}

impl BridgeSyncActivity {
    pub(crate) fn from_core(activity: bae_core::library::SyncActivity) -> Self {
        let moving = activity.moving();
        let needs_attention = activity.needs_attention();
        let bae_core::library::SyncActivity {
            failure,
            attention,
            syncing,
            uploads,
            downloads,
            retry_offered,
        } = activity;
        Self {
            moving,
            needs_attention,
            failure: failure.map(super::BridgeError::from_core),
            attention: super::BridgePendingRecords::from_core(attention),
            syncing,
            uploads: uploads.map(BridgeUploadsInFlight::from_core),
            downloads: downloads.map(BridgeDownloadsInFlight::from_core),
            retry_offered,
        }
    }
}

/// The files this device is sending now, out of its whole upload queue. The
/// bytes they have moved are the upload total's `KeyedValueStream`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadsInFlight {
    /// The list's line: the counts sending and waiting, a zero dropped.
    pub summary_parts: Vec<super::BridgeCountLabel>,
}

impl BridgeUploadsInFlight {
    fn from_core(uploads: bae_core::library::UploadsInFlight) -> Self {
        Self {
            summary_parts: uploads
                .summary_parts()
                .into_iter()
                .map(super::BridgeCountLabel::from_core)
                .collect(),
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

/// What the person's retry came to: the first sync pass after the request.
/// Mirrors `bae_core::library::SyncRetryOutcome`.
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
    pub(crate) fn from_core(outcome: bae_core::library::SyncRetryOutcome) -> Self {
        match outcome {
            bae_core::library::SyncRetryOutcome::Synced => Self::Synced,
            bae_core::library::SyncRetryOutcome::Failed(error) => Self::Failed {
                error: super::BridgeError::from_core(error),
            },
            bae_core::library::SyncRetryOutcome::CoolingDown { until } => {
                Self::CoolingDown { until }
            }
            bae_core::library::SyncRetryOutcome::NotSyncing => Self::NotSyncing,
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
