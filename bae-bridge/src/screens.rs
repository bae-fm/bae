//! The only place a screen subscription is built. Every value a screen shows
//! comes from a coven live query (core's typed wrappers over one) or from a
//! `KeyedValueStream`, and reaches the app by pull: the app awaits each
//! value's `next()`. Nothing here takes a channel or a closure loop.
//!
//! This file holds the pull every screen read answers through
//! ([`PulledRead`] and `pulled_read!`) and the live queries a screen follows
//! whole; the submodules hold the reads a screen moves in place (lists,
//! searches, details, the Storage Manager, the queue's tail, the import list)
//! and the streams (fast values, and the steps of an open, a setup call, a
//! close or an apply of a catalog release).
//! `AppHandle`'s `subscribe_*` and `feed_*` calls hand out what is built
//! here, and nothing else builds a pull object.

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
    bae_views::LiveLibraryBrowse<Projection, Snapshot> => Snapshot,
    over [Projection: Clone + PartialEq + Send + Sync + 'static, Snapshot: Send + 'static]
);
live_query_read!(bae_views::LiveLibrarySearch => bae_views::LibrarySearchSnapshot);
live_query_read!(bae_views::LiveArtistSearch => bae_views::ArtistSearchSnapshot);
live_query_read!(bae_library::LiveAlbumSelection => bae_library::AlbumSelectionSnapshot);
#[cfg(desktop)]
live_query_read!(bae_library::LiveLibraryStatus => bae_library::LibraryStatusSnapshot);

/// A read core's task produces, whose request moves in place.
impl<Request, Value> PulledRead for bae_library::LiveRead<Request, Value>
where
    Request: PartialEq + Send + Sync + 'static,
    Value: Send + 'static,
{
    type Value = Value;

    async fn next(&self) -> Result<Value, BridgeError> {
        bae_library::LiveRead::next(self).await.map_err(detail::live_read_error)
    }

    async fn cancel(&self) -> Result<(), BridgeError> {
        bae_library::LiveRead::cancel(self);
        Ok(())
    }
}

/// A screen's read as the app pulls it: `$object` holds `$inner`, a
/// [`PulledRead`], each pulled value converted to `$bridge` by `$convert`,
/// and `$make` builds it. With `when present`, `$convert` answers an
/// `Option`, and a pull passes over each value it answers `None` for,
/// waiting for the next. A screen that moves its read in place adds its own
/// exported calls on `$object`.
macro_rules! pulled_read {
    (
        $(#[$meta:meta])*
        $object:ident: $inner:ty => $bridge:ty, converted by $convert:expr, built by $make:ident
    ) => {
        pulled_read! {
            $(#[$meta])*
            $object: $inner => $bridge,
            converted by |value| Some(($convert)(value)), when present,
            built by $make
        }
    };
    (
        $(#[$meta:meta])*
        $object:ident: $inner:ty => $bridge:ty,
        converted by $convert:expr, when present,
        built by $make:ident
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

        pulled_read_calls! { $object => $bridge, converted by $convert, when present }
    };
}

/// `next` and `cancel` on `$object`, whose `inner` is its [`PulledRead`],
/// for an object that holds more than its read; [`pulled_read!`] makes them
/// for every other.
macro_rules! pulled_read_calls {
    ($object:ident => $bridge:ty, converted by $convert:expr) => {
        pulled_read_calls! {
            $object => $bridge, converted by |value| Some(($convert)(value)), when present
        }
    };
    ($object:ident => $bridge:ty, converted by $convert:expr, when present) => {
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
                        loop {
                            let value = crate::screens::PulledRead::next(&self.inner).await?;
                            if let Some(value) = ($convert)(value) {
                                return Ok(value);
                            }
                        }
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

/// One requested window of a list, with the rows that landed in it, named
/// for its row.
macro_rules! browse_window {
    ($(#[$meta:meta])* window: $window:ident, row: $core_row:ty => $bridge_row:ty,) => {
        $(#[$meta])*
        #[derive(Debug, Clone, uniffi::Record)]
        pub struct $window {
            pub window: crate::types::BridgeLibraryPageWindow,
            pub rows: Vec<$bridge_row>,
        }

        impl $window {
            fn from_core(window: bae_store::LibraryBrowseWindow<$core_row>) -> Self {
                Self {
                    window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                    rows: window.rows.into_iter().map(<$bridge_row>::from_core).collect(),
                }
            }
        }
    };
}

/// The windows a screen asks a list for, as core reads them.
fn core_windows(
    windows: Vec<crate::types::BridgeLibraryPageWindow>,
) -> bae_store::LibraryPageWindows {
    windows.into_iter().map(crate::types::BridgeLibraryPageWindow::into_core).collect()
}

// The submodules come after the macros above, which they use.
#[cfg(desktop)]
mod applying;
mod browse;
mod closing;
mod detail;
#[cfg(desktop)]
mod files;
#[cfg(desktop)]
mod import_list;
mod queue;
mod search;
mod setup;
mod storage;
mod streams;

#[cfg(desktop)]
pub use applying::*;
pub use browse::*;
pub use closing::*;
pub use detail::*;
#[cfg(desktop)]
pub use files::*;
#[cfg(desktop)]
pub use import_list::*;
pub use queue::*;
pub use search::*;
pub use setup::*;
pub use storage::*;
pub(crate) use streams::*;

pulled_read! {
    /// What playback is doing, apart from where it is in what it plays. A
    /// value whose playing track the library no longer holds is passed over:
    /// the value before it stands until playback moves off the track.
    PlaybackValuesSubscription:
        LiveValues<Option<bae_playback::NowPlayingValues>> => crate::types::BridgePlaybackValues,
    converted by |values: Option<bae_playback::NowPlayingValues>| {
        values.map(crate::types::BridgePlaybackValues::from_core)
    },
    when present,
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

pulled_read! {
    /// Each time tracks go into Up Next, with how many: one value per
    /// addition, none before the first since playback started with the
    /// library.
    QueueAdditionsSubscription:
        LiveValues<Option<bae_playback::QueueAddition>> => crate::types::BridgeQueueAddition,
    converted by |addition: Option<bae_playback::QueueAddition>| {
        addition.map(crate::types::BridgeQueueAddition::from_core)
    },
    when present,
    built by queue_additions
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
    /// What one candidate has in flight; `None` while nothing is.
    CandidateRuntimeSubscription:
        LiveValues<Option<bae_import::CandidateRuntimeSnapshot>>
            => Option<crate::types::BridgeCandidateRuntimeSnapshot>,
    converted by |runtime: Option<bae_import::CandidateRuntimeSnapshot>| {
        runtime.map(crate::types::BridgeCandidateRuntimeSnapshot::from_core)
    },
    built by candidate_runtime
}

#[cfg(feature = "cast")]
pulled_read! {
    /// The receivers the cast picker lists, by name.
    CastDevicesSubscription:
        LiveValues<Vec<bae_playback::renderer::CastDevice>> => Vec<crate::types::BridgeCastDevice>,
    converted by |devices: Vec<bae_playback::renderer::CastDevice>| {
        devices.into_iter().map(crate::types::BridgeCastDevice::from_core).collect::<Vec<_>>()
    },
    built by cast_devices
}

pulled_read! {
    /// This device's preferences for the library, as the settings screens
    /// show them.
    ConfigSubscription: LiveValues<bae_preferences::Config> => crate::types::BridgeConfig,
    converted by |config: bae_preferences::Config| crate::types::BridgeConfig::from_core(&config),
    built by config
}

pulled_read! {
    /// The queue as its screens show it.
    QueueSubscription:
        LiveValues<bae_playback::ResolvedQueueSnapshot> => crate::types::BridgeQueueSnapshot,
    converted by crate::types::BridgeQueueSnapshot::from_core,
    built by queue
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
    /// The store's members and their devices.
    MembersSubscription:
        LiveValues<bae_people::membership::Members> => crate::types::BridgeMembership,
    converted by crate::types::BridgeMembership::from_core,
    built by members
}

pulled_read! {
    /// This device's open invites, each with what coven holds its work for.
    InvitesSubscription:
        LiveValues<Vec<bae_views::OpenInvite>> => Vec<crate::types::BridgeInvite>,
    converted by |invites: Vec<bae_views::OpenInvite>| {
        invites.into_iter().map(crate::types::BridgeInvite::from_core).collect::<Vec<_>>()
    },
    built by invites
}

pulled_read! {
    /// The admin changes this device asked for, each until the person
    /// dismisses its outcome.
    AdminChangesSubscription:
        LiveValues<Vec<bae_people::admin_change::AdminChange>>
            => Vec<crate::types::BridgeAdminChange>,
    converted by |changes: Vec<bae_people::admin_change::AdminChange>| {
        changes.into_iter().map(crate::types::BridgeAdminChange::from_core).collect::<Vec<_>>()
    },
    built by admin_changes
}

pulled_read! {
    /// This person's device that joined after Add Device made its code; none
    /// until one does.
    AddedDeviceSubscription:
        LiveValues<Option<bae_people::membership::ActiveDevice>>
            => Option<crate::types::BridgeActiveDevice>,
    converted by |device: Option<bae_people::membership::ActiveDevice>| {
        device.map(crate::types::BridgeActiveDevice::from_core)
    },
    built by added_device
}

pulled_read! {
    /// Every person's profile.
    ProfilesSubscription:
        LiveValues<Vec<bae_people::people::PersonProfile>> => Vec<crate::types::BridgePersonProfile>,
    converted by |profiles: Vec<bae_people::people::PersonProfile>| {
        profiles.into_iter().map(crate::types::BridgePersonProfile::from_core).collect::<Vec<_>>()
    },
    built by profiles
}

pulled_read! {
    /// The profile this person asked for, until it is written: none before
    /// they ask and once it is.
    RequestedProfileSubscription:
        LiveValues<Option<bae_people::people::RequestedProfile>>
            => Option<crate::types::BridgeRequestedProfile>,
    converted by |requested: Option<bae_people::people::RequestedProfile>| {
        requested.map(crate::types::BridgeRequestedProfile::from_core)
    },
    built by requested_profile
}

pulled_read! {
    /// The library's name.
    LibraryNameSubscription: LiveValues<String> => String,
    converted by |name: String| name,
    built by library_name
}

#[cfg(desktop)]
pulled_read! {
    /// Where each watched folder's scan stands.
    FolderScansSubscription:
        LiveValues<bae_import::FolderScanProgress> => crate::types::BridgeFolderScans,
    converted by crate::types::BridgeFolderScans::from_core,
    built by folder_scans
}
