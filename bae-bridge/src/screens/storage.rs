//! What the Storage Manager pulls beside its list's windows, count and
//! positions: the rows of its selection, and the transfer queues holding work.

use crate::types::{BridgeError, BridgeStorageRow};

/// The rows of the selected releases the Storage Manager list holds, in a
/// window or not, and the storage actions every one of them allows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageSelection {
    pub selected: Vec<BridgeStorageRow>,
    pub selection_actions: Vec<crate::types::BridgeReleaseStorageAction>,
}

/// The transfer queues holding files waiting or on the move, each with
/// whether it is paused; the download queue (pins and moves-to-this-device) and the
/// output queue name the release they run. Mirrors `StorageQueues`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeStorageQueues {
    pub uploads: Option<BridgeUploadQueueRun>,
    pub downloads: Option<BridgeReleaseQueueRun>,
    pub outputs: Option<BridgeReleaseQueueRun>,
}

/// The upload queue while it holds files. Mirrors `UploadQueueRun`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadQueueRun {
    /// What holds the queue back.
    pub wait: Option<crate::types::BridgeUploadQueueWait>,
}

/// A one-release-at-a-time queue while it holds work. Mirrors
/// `ReleaseQueueRun`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseQueueRun {
    /// What holds the queue back.
    pub wait: Option<crate::types::BridgeQueueWait>,
    pub active: Option<String>,
}

impl BridgeStorageQueues {
    fn from_core(queues: bae_views::StorageQueues) -> Self {
        let release = |run: bae_views::ReleaseQueueRun| BridgeReleaseQueueRun {
            wait: run.wait.map(crate::types::BridgeQueueWait::from_core),
            active: run.active,
        };
        Self {
            uploads: queues.uploads.map(|run| BridgeUploadQueueRun {
                wait: run.wait.map(crate::types::BridgeUploadQueueWait::from_core),
            }),
            downloads: queues.downloads.map(release),
            outputs: queues.outputs.map(release),
        }
    }
}

impl BridgeStorageSelection {
    fn from_core(selection: bae_views::StorageSelection) -> Self {
        Self {
            selected: selection.selected.into_iter().map(BridgeStorageRow::from_core).collect(),
            selection_actions: selection
                .selection_actions
                .into_iter()
                .map(crate::types::BridgeReleaseStorageAction::from_core)
                .collect(),
        }
    }
}

pulled_read! {
    /// The rows of the selected releases the Storage Manager list holds, read
    /// live: the selection moves in place with `set_selected`, and starts
    /// empty.
    StorageSelectionSubscription:
        bae_app::live_reads::LiveStorageSelection => BridgeStorageSelection,
    converted by BridgeStorageSelection::from_core,
    built by storage_selection
}

#[uniffi::export]
impl StorageSelectionSubscription {
    /// Read the rows of the `selected` releases the list holds from now on.
    pub fn set_selected(&self, selected: Vec<String>) -> Result<(), BridgeError> {
        self.inner
            .set(selected.into_iter().collect())
            .map(|_revision| ())
            .map_err(BridgeError::from)
    }
}

pulled_read! {
    /// The transfer queues holding work, as the Storage Manager shows them,
    /// read live.
    StorageQueuesSubscription:
        bae_store::live_query::LiveValues<bae_views::StorageQueues> => BridgeStorageQueues,
    converted by BridgeStorageQueues::from_core,
    built by storage_queues
}
