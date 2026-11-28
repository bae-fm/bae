//! What a release's storage offers its person, decided in SQL from where its
//! files are: a fragment every read of a release's storage state spreads.

use bae_types::release_storage::ReleaseStorageAction;

/// The storage actions a release, or every release of a selection, offers,
/// and whether Cancel is the only action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbStorageActionFlags {
    pub upload: bool,
    pub pin: bool,
    pub unpin: bool,
    pub move_to_this_device: bool,
    /// Whether Cancel is the only action offered: a release is
    /// mid-transition, or its move to storage failed and cancelling ends the
    /// move.
    pub cancel_only: bool,
}

impl DbStorageActionFlags {
    /// The actions offered, in the order the screens list them; none for a
    /// selection of no releases.
    pub fn list(&self) -> Vec<ReleaseStorageAction> {
        [
            (self.upload, ReleaseStorageAction::Upload),
            (self.pin, ReleaseStorageAction::Pin),
            (self.unpin, ReleaseStorageAction::Unpin),
            (self.move_to_this_device, ReleaseStorageAction::MoveToThisDevice),
        ]
        .into_iter()
        .filter_map(|(offered, action)| offered.then_some(action))
        .collect()
    }

    /// No action: what a selection of no releases allows.
    pub(crate) fn none() -> Self {
        Self {
            upload: false,
            pin: false,
            unpin: false,
            move_to_this_device: false,
            cancel_only: false,
        }
    }
}

/// The storage actions a release offers and whether it is mid-transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbStorageActions {
    pub offered: DbStorageActionFlags,
    /// Whether files wait to upload or a transfer runs: the release offers
    /// only cancelling then.
    pub transitioning: bool,
}

// The storage actions each release offers and whether it is mid-transition,
// by `actions_release_id`: a release mid-transition (files waiting in the
// upload queue, or its download entry running or a move to this device
// waiting) offers none. A release not in storage offers Upload while storage
// is connected, and a release in storage offers Pin or Unpin by whether this
// device keeps every file, and Move to This Device. Nothing here asks what coven would refuse: coven refuses a move of a
// file another device holds, or a pin or a move to this device with no storage,
// and the app shows coven's reason. coven queues a move with no storage
// connected rather than refusing it, so the offer of Upload waits for storage.
coven::fragment!(
    ReleaseStorageActions {
        can_upload: bool,
        can_pin: bool,
        can_unpin: bool,
        can_move_to_this_device: bool,
        transitioning: bool,
        cancel_only: bool
    },
    r#"
    WITH offered AS (
        SELECT storage.release_id AS offered_release_id, storage.stored = 1 AS in_storage,
               storage.pinned, storage.failed_move_file IS NOT NULL AS move_failed,
               storage.queued_count > 0
                   OR COALESCE(queued.state = 'active'
                               OR (queued.kind = 'move_to_this_device'
                                   AND queued.state = 'queued'), 0) AS moving
        FROM @bae_records::release_storage::ReleaseStorage AS storage
        LEFT JOIN @bae_transfers::download_entry::QueuedDownloadEntry AS queued
               ON queued.release_id = storage.release_id)
    SELECT offered.offered_release_id AS actions_release_id,
           NOT offered.moving
               AND (SELECT sync.storage = 'connected' FROM coven_sync_status AS sync)
               AND NOT offered.in_storage AS can_upload,
           NOT offered.moving AND offered.in_storage AND NOT offered.pinned AS can_pin,
           NOT offered.moving AND offered.in_storage AND offered.pinned AS can_unpin,
           NOT offered.moving AND offered.in_storage AS can_move_to_this_device,
           offered.moving AS transitioning,
           offered.moving OR offered.move_failed AS cancel_only
    FROM offered
"#
);

impl ReleaseStorageActions {
    /// The actions the release offers.
    pub(crate) fn actions(&self) -> DbStorageActions {
        DbStorageActions {
            offered: DbStorageActionFlags {
                upload: self.can_upload,
                pin: self.can_pin,
                unpin: self.can_unpin,
                move_to_this_device: self.can_move_to_this_device,
                cancel_only: self.cancel_only,
            },
            transitioning: self.transitioning,
        }
    }
}
