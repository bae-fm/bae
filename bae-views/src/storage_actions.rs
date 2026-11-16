//! What a release's storage offers its person, decided in SQL from where its
//! files are: the columns every read of a release's storage state carries, and
//! what a selection of releases allows as one statement over them.

use bae_records::records::query::RELEASE_STORAGE_PINNED;
use bae_types::release_storage::ReleaseStorageAction;
use rusqlite::Row;

/// The storage actions a release, or every release of a selection, offers,
/// and whether Cancel is the only action, as [`storage_action_columns`] or
/// [`selection_action_columns`] reads them.
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

    /// The flags the action columns gave `row`.
    pub fn of_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            upload: row.get("can_upload")?,
            pin: row.get("can_pin")?,
            unpin: row.get("can_unpin")?,
            move_to_this_device: row.get("can_move_to_this_device")?,
            cancel_only: row.get("cancel_only")?,
        })
    }
}

/// The storage actions a release offers and whether it is mid-transition, as
/// [`storage_action_columns`] reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbStorageActions {
    pub offered: DbStorageActionFlags,
    /// Whether files wait to upload or a transfer runs: the release offers
    /// only cancelling then.
    pub transitioning: bool,
}

impl DbStorageActions {
    /// The actions [`storage_action_columns`] gave `row`.
    pub fn of_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            offered: DbStorageActionFlags::of_row(row)?,
            transitioning: row.get("transitioning")?,
        })
    }
}

/// Whether a release's files wait to upload or a transfer of them runs, for
/// the release `release` (an SQL expression naming its id) whose
/// `release_storage` row is `storage`: files wait in the upload queue, or its
/// download queue entry runs, or is a move to this device waiting. Starting
/// another storage action then would conflict with it.
fn transitioning_sql(release: &str) -> String {
    format!(
        "(COALESCE(storage.queued_count > 0, 0) \
          OR EXISTS (SELECT 1 FROM download_queue q WHERE q.release_id = {release} \
                     AND (q.state = 'active' \
                          OR (q.kind = 'move_to_this_device' AND q.state = 'queued'))))"
    )
}

/// The columns of the storage actions the release `release` (an SQL
/// expression naming its id) offers, beside its `release_storage` row
/// `storage`: one `can_<action>` for each, and `transitioning`.
///
/// Storage needs the library to sync; a release mid-transition offers none.
/// `Upload` moves the files of a release not in storage, from this device's
/// originals, so a release offers it only when each audio file is on this
/// device. A release in storage offers `Pin` or `Unpin` by whether this device
/// keeps every file, and `MoveToThisDevice`.
pub(crate) fn storage_action_columns(release: &str) -> String {
    let offered = format!(
        "(SELECT storage = 'connected' FROM coven_sync_status) AND NOT {transitioning}",
        transitioning = transitioning_sql(release),
    );
    let stored = "COALESCE(storage.stored = 1, 0)";
    let pinned = format!("COALESCE({RELEASE_STORAGE_PINNED}, 0)");
    let elsewhere =
        format!("EXISTS (SELECT 1 FROM release_audio_elsewhere WHERE release_id = {release})");
    format!(
        "({offered} AND NOT {stored} AND NOT {elsewhere}) AS can_upload, \
         ({offered} AND {stored} AND NOT {pinned}) AS can_pin, \
         ({offered} AND {stored} AND {pinned}) AS can_unpin, \
         ({offered} AND {stored}) AS can_move_to_this_device, \
         {transitioning} AS transitioning, \
         ({transitioning} OR COALESCE(storage.move_failed = 1, 0)) AS cancel_only",
        transitioning = transitioning_sql(release),
    )
}

/// The columns over rows carrying [`storage_action_columns`]: each action
/// every row offers (the least of its flags, none for no rows) and whether
/// Cancel is the only action one of the rows offers.
pub(crate) fn selection_action_columns() -> &'static str {
    "COALESCE(MIN(can_upload), 0) AS can_upload, \
     COALESCE(MIN(can_pin), 0) AS can_pin, \
     COALESCE(MIN(can_unpin), 0) AS can_unpin, \
     COALESCE(MIN(can_move_to_this_device), 0) AS can_move_to_this_device, \
      COALESCE(MAX(cancel_only), 0) AS cancel_only"
}
