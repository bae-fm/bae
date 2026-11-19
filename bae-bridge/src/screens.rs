//! The only place a screen subscription is built. Every value a screen shows
//! comes from a coven live query (core's typed wrappers over one), from
//! coven's byte watch beside the catalog image cache's changes (what a
//! device keeps), or from a `KeyedValueStream`, and reaches the app by pull: the app awaits each
//! value's `next()`. Nothing here takes a channel or a closure loop.
//!
//! This file holds the pull every screen read answers through ([`PulledRead`]
//! and `pulled_read!`) and the live queries a screen follows whole; the
//! submodules hold the reads whose request a screen changes on the same live
//! query (lists, searches, details, the Storage Manager, the queue's tail, the
//! import list) and the streams (fast values, and the steps of an open, a setup
//! call or a close). `AppHandle`'s `subscribe_*` and `feed_*` calls hand out
//! the pull objects this file and its submodules build, and no other code
//! builds one.

use std::future::Future;

use bae_store::live_query::LiveValues;

use crate::types::BridgeError;

/// A core read a screen pulls from, as the bridge answers the pulls: its
/// next value, or its failure as the bridge reports it, and its end.
pub(crate) trait PulledRead: Send + Sync + 'static {
    type Value: Send + 'static;

    /// The read's next value; waits until there is one.
    fn next(&self) -> impl Future<Output = Result<Self::Value, BridgeError>> + Send;

    /// End the read.
    fn cancel(&self) -> impl Future<Output = Result<(), BridgeError>> + Send;
}

/// A read over one coven live query: its errors are the query's, and its end
/// can't fail.
macro_rules! live_query_read {
    ($read:ty => $value:ty $(, over [$($generics:tt)*])?) => {
        impl$(<$($generics)*>)? PulledRead for $read {
            type Value = $value;

            async fn next(&self) -> Result<$value, BridgeError> {
                <$read>::next(self).await.map_err(BridgeError::from)
            }

            async fn cancel(&self) -> Result<(), BridgeError> {
                <$read>::cancel(self).await;
                Ok(())
            }
        }
    };
}

live_query_read!(LiveValues<T> => T, over [T: Clone + PartialEq + Send + Sync + 'static]);
live_query_read!(
    bae_store::live_query::LiveRead<Request, Value> => Value,
    over [Request: Send + Sync + 'static, Value: Send + 'static]
);

#[cfg(desktop)]
live_query_read!(
    bae_app::live_reads::LiveImportCandidate
        => bae_library::DetailSnapshot<bae_import::ImportCandidateDetail, bae_import::CandidateKey>
);

/// A screen's read as the app pulls it: `$object` holds `$inner`, a
/// [`PulledRead`], each pulled value converted to `$bridge` by `$convert`,
/// and `$make` builds it. Every value the read answers reaches the app. A
/// screen that moves its read in place adds its own exported calls on
/// `$object`.
macro_rules! pulled_read {
    (
        $(#[$meta:meta])*
        $object:ident: $inner:ty => $bridge:ty, converted by $convert:expr, built by $make:ident
    ) => {
        $(#[$meta])*
        #[derive(uniffi::Object)]
        pub struct $object {
            inner: $inner,
            runtime: crate::operation_runtime::OperationRuntime,
        }

        pub(crate) fn $make(
            inner: $inner,
            runtime: crate::operation_runtime::OperationRuntime,
        ) -> std::sync::Arc<$object> {
            std::sync::Arc::new($object { inner, runtime })
        }

        pulled_read_calls! { $object => $bridge, converted by $convert }
    };
}

/// `next` and `cancel` on `$object`, whose `inner` is its [`PulledRead`],
/// for an object that holds more than its read; [`pulled_read!`] makes them
/// for every other.
macro_rules! pulled_read_calls {
    ($object:ident => $bridge:ty, converted by $convert:expr) => {
        #[uniffi::export(async_runtime = "tokio", cancellable)]
        impl $object {
            /// The read's next value, the first at once. A failed read is
            /// answered as its error.
            pub async fn next(
                self: std::sync::Arc<Self>,
            ) -> Result<$bridge, crate::types::BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".next"), move || async move {
                        let value = crate::screens::PulledRead::next(&self.inner).await?;
                        Ok(($convert)(value))
                    })
                    .await
            }

            /// End the read.
            pub async fn cancel(
                self: std::sync::Arc<Self>,
            ) -> Result<(), crate::types::BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".cancel"), move || async move {
                        crate::screens::PulledRead::cancel(&self.inner).await
                    })
                    .await
            }
        }
    };
}

// The submodules come after the macros above, which they use.
#[cfg(desktop)]
mod applying;
mod browse;
mod closing;
#[cfg(desktop)]
mod cover_gallery;
mod detail;
#[cfg(desktop)]
mod files;
#[cfg(desktop)]
mod folder_images;
#[cfg(desktop)]
mod import_list;
mod lists;
mod search;
mod setup;
mod storage;
mod streams;

#[cfg(desktop)]
pub use applying::*;
pub use browse::*;
pub use closing::*;
#[cfg(desktop)]
pub use cover_gallery::*;
pub use detail::*;
#[cfg(desktop)]
pub use files::*;
#[cfg(desktop)]
pub use folder_images::*;
#[cfg(desktop)]
pub use import_list::*;
pub use lists::*;
pub use search::*;
pub use setup::*;
pub use storage::*;
pub(crate) use streams::*;

pulled_read! {
    /// What playback is doing, apart from where it is in what it plays: a
    /// track the library no longer holds shows as removed until playback
    /// moves off it.
    PlaybackValuesSubscription:
        LiveValues<bae_playback::NowPlayingValues> => crate::types::BridgePlaybackValues,
    converted by crate::types::BridgePlaybackValues::from_core,
    built by playback_values
}

pulled_read! {
    /// Where playback is, for a surface that moves the position on itself:
    /// the timeline now, then each time playback leaves the line it holds.
    PlaybackTimelineSubscription:
        LiveValues<bae_playback::PlaybackTimeline> => crate::types::BridgePlaybackTimeline,
    converted by crate::types::BridgePlaybackTimeline::from_core,
    built by playback_timeline
}

#[cfg(desktop)]
pulled_read! {
    /// How far the identifications and imports in flight have got.
    ImportCountsSubscription:
        LiveValues<bae_import::ImportCounts> => crate::types::BridgeImportCounts,
    converted by crate::types::BridgeImportCounts::from_core,
    built by import_counts
}

#[cfg(desktop)]
pulled_read! {
    /// The identify state one candidate's pane shows, and what it has in
    /// flight.
    CandidateRuntimeSubscription:
        LiveValues<bae_import::CandidateRuntimeView>
            => crate::types::BridgeCandidateRuntimeSnapshot,
    // The read passes a closure, not the function's path, because the
    // ownership check follows the closure into the functions it calls and
    // checks the conversion as read code.
    converted by |view: bae_import::CandidateRuntimeView| {
        crate::types::BridgeCandidateRuntimeSnapshot::from_core(view)
    },
    built by candidate_runtime
}

#[cfg(desktop)]
pulled_read! {
    /// The network read an apply of a catalog release waits on; `None` while
    /// it waits on none.
    CatalogApplyStepSubscription:
        LiveValues<Option<bae_import::CatalogApplyStep>>
            => Option<crate::types::BridgeCatalogApplyStep>,
    converted by |step: Option<bae_import::CatalogApplyStep>| {
        step.map(crate::types::BridgeCatalogApplyStep::from_core)
    },
    built by catalog_apply_step
}

#[cfg(desktop)]
pulled_read! {
    /// A release editor's session's draft, as the editor shows it; `None`
    /// once the session's release is gone.
    ReleaseEditDraftSubscription:
        LiveValues<Option<bae_import::ReleaseEditDraft>>
            => Option<crate::types::BridgeReleaseEditDraft>,
    converted by |draft: Option<bae_import::ReleaseEditDraft>| {
        draft.map(crate::types::BridgeReleaseEditDraft::from_core)
    },
    built by release_edit_draft
}

#[cfg(feature = "cast")]
pulled_read! {
    /// The receivers the cast picker lists, the cast-to receiver apart from
    /// the rest, which are by name.
    CastReceiversSubscription:
        LiveValues<bae_playback::renderer::CastReceiverList>
            => crate::types::BridgeCastReceiverList,
    converted by crate::types::BridgeCastReceiverList::from_core,
    built by cast_receivers
}

#[cfg(desktop)]
pulled_read! {
    /// The Subsonic server besides the settings: what it is doing, the
    /// networks to choose from, and whether a password is saved.
    SubsonicSubscription:
        LiveValues<bae_subsonic::SubsonicView> => crate::types::BridgeSubsonicView,
    converted by crate::types::BridgeSubsonicView::from_core,
    built by subsonic
}

pulled_read! {
    /// What this device keeps for the library: coven's byte measurements and
    /// the catalog image cache's, now and after each change.
    DeviceStorageSubscription:
        LiveValues<bae_library::DeviceStorage> => crate::types::BridgeDeviceStorage,
    converted by crate::types::BridgeDeviceStorage::from_core,
    built by device_storage
}

pulled_read! {
    /// This device's preferences for the library, as the settings screens
    /// show them.
    ConfigSubscription: LiveValues<bae_preferences::Config> => crate::types::BridgeConfig,
    converted by |config: bae_preferences::Config| crate::types::BridgeConfig::from_core(&config),
    built by config
}

pulled_read! {
    /// One save preset as the format editor shows it, or none once the
    /// person deletes it.
    SavePresetSubscription:
        LiveValues<Option<bae_preferences::SavePreset>> => Option<crate::types::BridgeSavePreset>,
    converted by |preset: Option<bae_preferences::SavePreset>| {
        preset.as_ref().map(crate::types::BridgeSavePreset::from_core)
    },
    built by save_preset
}

pulled_read! {
    /// The queue as its screens show it.
    QueueSubscription:
        LiveValues<bae_playback::ResolvedQueueSnapshot> => crate::types::BridgeQueueSnapshot,
    converted by crate::types::BridgeQueueSnapshot::from_core,
    built by queue
}

pulled_read! {
    /// The tracks after the playing one as the system media session lists
    /// them.
    SessionUpNextSubscription:
        LiveValues<Vec<bae_playback::QueueItem>> => Vec<crate::types::BridgeQueueEntry>,
    converted by |items: Vec<bae_playback::QueueItem>| {
        items.into_iter().map(crate::types::BridgeQueueEntry::from_core).collect()
    },
    built by session_up_next
}

pulled_read! {
    /// How many downloads are in each phase, and whether the queue is paused.
    DownloadCountsSubscription:
        LiveValues<bae_transfers::ReleaseQueueCounts> => crate::types::BridgeDownloadCounts,
    converted by crate::types::BridgeDownloadCounts::from_core,
    built by download_counts
}

pulled_read! {
    /// What sync is doing, what it waits on, and why a start failed.
    SyncStatusSubscription:
        LiveValues<bae_sync_status::SyncStatusSnapshot> => crate::types::BridgeSyncStatusSnapshot,
    converted by crate::types::BridgeSyncStatusSnapshot::from_core,
    built by sync_status
}

pulled_read! {
    /// Everything sync is doing: what needs the person, and the transfers
    /// moving bytes.
    SyncActivitySubscription:
        LiveValues<bae_views::SyncActivity> => crate::types::BridgeSyncActivity,
    converted by crate::types::BridgeSyncActivity::from_core,
    built by sync_activity
}

pulled_read! {
    /// Whether the person paused uploads.
    UploadsPausedSubscription: LiveValues<bool> => bool,
    converted by |paused: bool| paused,
    built by uploads_paused
}

pulled_read! {
    /// Whether files move only on unmetered networks.
    UnmeteredOnlySubscription: LiveValues<bool> => bool,
    converted by |unmetered_only: bool| unmetered_only,
    built by unmetered_only
}

pulled_read! {
    /// The store's members and their devices.
    MembersSubscription:
        LiveValues<bae_people::membership::Members> => crate::types::BridgeMembership,
    converted by crate::types::BridgeMembership::from_core,
    built by members
}

pulled_read! {
    /// The person using this device, the other people, and how many people and
    /// devices the library has.
    PeopleSubscription:
        LiveValues<bae_people::people::PeopleView> => crate::types::BridgePeople,
    converted by crate::types::BridgePeople::from_core,
    built by people
}

pulled_read! {
    /// One person of the library, by member id: none once they have left.
    PersonSubscription:
        LiveValues<Option<bae_people::people::Person>> => Option<crate::types::BridgePerson>,
    converted by |person: Option<bae_people::people::Person>| {
        person.map(crate::types::BridgePerson::from_core)
    },
    built by person
}

pulled_read! {
    /// This device's open invites, each with what coven holds its work for,
    /// and the invites a sharing refusal ended, in the order they were made.
    InvitesSubscription:
        LiveValues<Vec<bae_views::InviteEntry>> => Vec<crate::types::BridgeInviteEntry>,
    converted by |invites: Vec<bae_views::InviteEntry>| {
        invites.into_iter().map(crate::types::BridgeInviteEntry::from_core).collect::<Vec<_>>()
    },
    built by invites
}

pulled_read! {
    /// One of this device's open invites, or none.
    OpenInviteSubscription:
        LiveValues<Option<bae_views::OpenInvite>> => Option<crate::types::BridgeInvite>,
    converted by |invite: Option<bae_views::OpenInvite>| {
        invite.map(crate::types::BridgeInvite::from_core)
    },
    built by open_invite
}

pulled_read! {
    /// The admin changes this device asked for, each until the person
    /// dismisses its outcome.
    AdminChangesSubscription:
        LiveValues<bae_people::admin_change::AdminChanges> => crate::types::BridgeAdminChanges,
    converted by crate::types::BridgeAdminChanges::from_core,
    built by admin_changes
}

pulled_read! {
    /// Where the pairing Add Device's sheet started is: waiting for a
    /// request, approved, ended, or forgotten once the sheet has ended it.
    AddDeviceStatusSubscription:
        LiveValues<Option<coven::AddDeviceStatus>> => crate::handle::BridgeAddDeviceStatus,
    converted by crate::handle::BridgeAddDeviceStatus::from_core,
    built by add_device_status
}

pulled_read! {
    /// The artist sections of the album grid grouped by artist: each artist's
    /// heading and where its albums start in the grid's rows.
    AlbumSectionsSubscription:
        LiveValues<Vec<bae_views::LibraryBrowseSection>> => Vec<crate::types::BridgeLibraryBrowseSection>,
    converted by |sections: Vec<bae_views::LibraryBrowseSection>| {
        sections
            .into_iter()
            .map(crate::types::BridgeLibraryBrowseSection::from_core)
            .collect::<Vec<_>>()
    },
    built by album_sections
}

pulled_read! {
    /// Where an album's card is in the album grid, or none once the grid
    /// shows no such card.
    AlbumPlaceSubscription:
        LiveValues<Option<bae_views::album::AlbumPlace>> => Option<crate::types::BridgeAlbumPlace>,
    converted by |place: Option<bae_views::album::AlbumPlace>| {
        place.map(crate::types::BridgeAlbumPlace::from_core)
    },
    built by album_place
}

pulled_read! {
    /// Every playlist this device holds.
    PlaylistsSubscription:
        LiveValues<bae_views::playlists::PlaylistListing> => crate::types::BridgePlaylistListing,
    converted by crate::types::BridgePlaylistListing::from_core,
    built by playlists
}

pulled_read! {
    /// The library's name.
    LibraryNameSubscription: LiveValues<String> => String,
    converted by |name: String| name,
    built by library_name
}

#[cfg(desktop)]
pulled_read! {
    /// Where each watched folder's scan stands, with an alert for each
    /// problem not among the alerts the app raised. The subscription keeps
    /// one live query while it is open, and the app sets the alerts it
    /// raised on it as it raises them; it starts with none.
    FolderScansSubscription:
        bae_app::live_reads::LiveFolderScans => crate::types::BridgeFolderScans,
    converted by crate::types::BridgeFolderScans::from_core,
    built by folder_scans
}

#[cfg(desktop)]
#[uniffi::export]
impl FolderScansSubscription {
    /// Read with `raised` as every alert the app raised from now on: the
    /// folder scans list no alert for a problem among them.
    pub fn set_raised(
        &self,
        raised: Vec<crate::types::BridgeFolderScanAlert>,
    ) -> Result<(), crate::types::BridgeError> {
        let raised = raised
            .into_iter()
            .map(crate::types::BridgeFolderScanAlert::into_core)
            .collect::<Result<bae_import::RaisedScanAlerts, _>>()?;
        self.inner.set(raised).map(|_revision| ()).map_err(crate::types::BridgeError::from)
    }
}

#[cfg(desktop)]
pulled_read! {
    /// The folders being scanned, each with how many folders its scan has
    /// stored.
    ScanningFoldersSubscription:
        LiveValues<Vec<bae_import::ScanningFolder>> => Vec<crate::types::BridgeScanningFolder>,
    converted by |folders: Vec<bae_import::ScanningFolder>| {
        folders.into_iter().map(crate::types::BridgeScanningFolder::from_core).collect::<Vec<_>>()
    },
    built by scanning_folders
}
