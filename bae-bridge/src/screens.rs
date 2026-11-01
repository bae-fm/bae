//! The only place a screen subscription is built. Every value a screen shows
//! comes from a coven live query (core's typed wrappers over one) or from a
//! `KeyedValueStream`, and reaches the app by pull: the app awaits each
//! value's `next()`. Nothing here takes a channel or a closure loop.
//!
//! This file holds the live queries a screen follows whole; the submodules
//! hold the reads a screen moves in place (lists, searches, details, the
//! Storage Manager, the queue's tail, the import list) and the streams
//! (fast values, and the steps of an open or a setup call).
//! `AppHandle`'s `subscribe_*` and `feed_*` calls hand out what is built
//! here, and nothing else builds a pull object.

use std::sync::Arc;

use crate::types::BridgeError;

mod browse;
mod detail;
#[cfg(desktop)]
mod files;
#[cfg(feature = "desktop")]
mod import_list;
mod queue;
mod search;
mod setup;
mod storage;
mod streams;

pub use browse::*;
pub use detail::*;
#[cfg(desktop)]
pub use files::*;
#[cfg(feature = "desktop")]
pub use import_list::*;
pub use queue::*;
pub use search::*;
pub use setup::*;
pub use storage::*;
pub(crate) use streams::*;

/// A live query's values as the app pulls them: `$object` holds core's
/// [`LiveValues`](bae_core::library::LiveValues) of `$core`, each pulled
/// value converted to `$bridge` by `$convert`, and `$make` builds it. With
/// `when present`, `$convert` answers an `Option`, and a pull passes over
/// each value it answers `None` for, waiting for the next.
macro_rules! pulled_values {
    (
        $(#[doc = $doc:literal])*
        $object:ident: $core:ty => $bridge:ty, converted by $convert:expr, built by $make:ident
    ) => {
        pulled_values! {
            $(#[doc = $doc])*
            $object: $core => $bridge,
            converted by |value: $core| Some(($convert)(value)), when present,
            built by $make
        }
    };
    (
        $(#[doc = $doc:literal])*
        $object:ident: $core:ty => $bridge:ty,
        converted by $convert:expr, when present,
        built by $make:ident
    ) => {
        $(#[doc = $doc])*
        #[derive(uniffi::Object)]
        pub struct $object {
            values: bae_core::library::LiveValues<$core>,
            runtime: crate::operation_runtime::OperationRuntime,
        }

        #[uniffi::export(async_runtime = "tokio", cancellable)]
        impl $object {
            /// The first value at once, then one after each write that
            /// changes it. A failed read is answered as an error; the next
            /// pull waits for the read after it.
            pub async fn next(self: Arc<Self>) -> Result<$bridge, BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".next"), move || async move {
                        loop {
                            let value = self.values.next().await.map_err(values_error)?;
                            if let Some(value) = ($convert)(value) {
                                return Ok(value);
                            }
                        }
                    })
                    .await
            }

            /// End the subscription: a waiting pull answers that it was
            /// cancelled, and so does every later one.
            pub async fn cancel(self: Arc<Self>) -> Result<(), BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".cancel"), move || async move {
                        self.values.cancel().await;
                        Ok(())
                    })
                    .await
            }
        }

        pub(crate) fn $make(
            values: bae_core::library::LiveValues<$core>,
            runtime: crate::operation_runtime::OperationRuntime,
        ) -> Arc<$object> {
            Arc::new($object { values, runtime })
        }
    };
}

fn values_error(error: bae_core::library::LiveValuesError) -> BridgeError {
    match error {
        bae_core::library::LiveValuesError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LiveValuesError::Query(error) => BridgeError::database_query(error),
    }
}

pulled_values! {
    /// What playback is doing, apart from where it is in what it plays. A
    /// value whose playing track the library no longer holds is passed over:
    /// the value before it stands until playback moves off the track.
    PlaybackValuesSubscription:
        Option<bae_core::playback::NowPlayingValues> => crate::types::BridgePlaybackValues,
    converted by |values: Option<bae_core::playback::NowPlayingValues>| {
        values.map(crate::types::BridgePlaybackValues::from_core)
    },
    when present,
    built by playback_values
}

pulled_values! {
    /// Where playback is, for a surface that moves the position on itself:
    /// the timeline now, then each time playback leaves the line it holds.
    PlaybackTimelineSubscription:
        bae_core::playback::PlaybackTimeline => crate::types::BridgePlaybackTimeline,
    converted by crate::types::BridgePlaybackTimeline::from_core,
    built by playback_timeline
}

pulled_values! {
    /// Each time tracks go into Up Next, with how many: one value per
    /// addition, none before the first since playback started with the
    /// library.
    QueueAdditionsSubscription:
        Option<bae_core::queue::QueueAddition> => crate::types::BridgeQueueAddition,
    converted by |addition: Option<bae_core::queue::QueueAddition>| {
        addition.map(crate::types::BridgeQueueAddition::from_core)
    },
    when present,
    built by queue_additions
}

#[cfg(feature = "desktop")]
pulled_values! {
    /// How far the identifications and imports in flight have got.
    ImportCountsSubscription: bae_core::import::ImportCounts => crate::types::BridgeImportCounts,
    converted by crate::types::BridgeImportCounts::from_core,
    built by import_counts
}

#[cfg(feature = "desktop")]
pulled_values! {
    /// What one candidate has in flight; `None` while nothing is.
    CandidateRuntimeSubscription:
        Option<bae_core::import::CandidateRuntimeSnapshot>
            => Option<crate::types::BridgeCandidateRuntimeSnapshot>,
    converted by |runtime: Option<bae_core::import::CandidateRuntimeSnapshot>| {
        runtime.map(crate::types::BridgeCandidateRuntimeSnapshot::from_core)
    },
    built by candidate_runtime
}

#[cfg(feature = "cast")]
pulled_values! {
    /// The receivers the cast picker lists, by name.
    CastDevicesSubscription:
        Vec<bae_core::renderer::CastDevice> => Vec<crate::types::BridgeCastDevice>,
    converted by |devices: Vec<bae_core::renderer::CastDevice>| {
        devices.into_iter().map(crate::types::BridgeCastDevice::from_core).collect::<Vec<_>>()
    },
    built by cast_devices
}

pulled_values! {
    /// This device's preferences for the library, as the settings screens
    /// show them.
    ConfigSubscription: bae_core::config::Config => crate::types::BridgeConfig,
    converted by |config: bae_core::config::Config| crate::types::BridgeConfig::from_core(&config),
    built by config
}

pulled_values! {
    /// The queue as its screens show it.
    QueueSubscription: bae_core::queue::ResolvedQueueSnapshot => crate::types::BridgeQueueSnapshot,
    converted by crate::types::BridgeQueueSnapshot::from_core,
    built by queue
}

pulled_values! {
    /// How many downloads are in each phase, and whether the queue is paused.
    DownloadCountsSubscription:
        bae_core::library::ReleaseQueueCounts => crate::types::BridgeDownloadCounts,
    converted by crate::types::BridgeDownloadCounts::from_core,
    built by download_counts
}

pulled_values! {
    /// What sync is doing, what it waits on, and why a start failed.
    SyncStatusSubscription:
        bae_core::library::SyncStatusSnapshot => crate::types::BridgeSyncStatusSnapshot,
    converted by crate::types::BridgeSyncStatusSnapshot::from_core,
    built by sync_status
}

pulled_values! {
    /// Everything sync is doing: what needs the person, and the transfers
    /// moving bytes.
    SyncActivitySubscription:
        bae_core::library::SyncActivity => crate::types::BridgeSyncActivity,
    converted by crate::types::BridgeSyncActivity::from_core,
    built by sync_activity
}

pulled_values! {
    /// Whether the person paused uploads.
    UploadsPausedSubscription: bool => bool,
    converted by |paused: bool| paused,
    built by uploads_paused
}

pulled_values! {
    /// The store's members and their devices.
    MembersSubscription:
        bae_core::sync::membership::Membership => crate::types::BridgeMembership,
    converted by crate::types::BridgeMembership::from_core,
    built by members
}

pulled_values! {
    /// This device's open invites, each with its request once one has
    /// arrived.
    InvitesSubscription: Vec<coven::Invite> => Vec<crate::types::BridgeInvite>,
    converted by |invites: Vec<coven::Invite>| {
        invites.into_iter().map(crate::types::BridgeInvite::from_core).collect::<Vec<_>>()
    },
    built by invites
}

pulled_values! {
    /// This person's device that joined after Add Device made its code; none
    /// until one does.
    AddedDeviceSubscription:
        Option<bae_core::sync::membership::ActiveDevice>
            => Option<crate::types::BridgeActiveDevice>,
    converted by |device: Option<bae_core::sync::membership::ActiveDevice>| {
        device.map(crate::types::BridgeActiveDevice::from_core)
    },
    built by added_device
}

pulled_values! {
    /// Every person's profile.
    ProfilesSubscription:
        Vec<bae_core::people::PersonProfile> => Vec<crate::types::BridgePersonProfile>,
    converted by |profiles: Vec<bae_core::people::PersonProfile>| {
        profiles.into_iter().map(crate::types::BridgePersonProfile::from_core).collect::<Vec<_>>()
    },
    built by profiles
}

pulled_values! {
    /// This person's ratings.
    RatingsSubscription:
        Vec<bae_core::collections::Rating> => Vec<crate::types::BridgeRating>,
    converted by |ratings: Vec<bae_core::collections::Rating>| {
        ratings.into_iter().map(crate::types::BridgeRating::from_core).collect::<Vec<_>>()
    },
    built by ratings
}

pulled_values! {
    /// The playlists this person sees.
    PlaylistsSubscription:
        Vec<bae_core::collections::Playlist> => Vec<crate::types::BridgePlaylist>,
    converted by |playlists: Vec<bae_core::collections::Playlist>| {
        playlists.into_iter().map(crate::types::BridgePlaylist::from_core).collect::<Vec<_>>()
    },
    built by playlists
}

pulled_values! {
    /// One playlist's entries, in order.
    PlaylistEntriesSubscription:
        Vec<bae_core::collections::PlaylistEntry> => Vec<crate::types::BridgePlaylistEntry>,
    converted by |entries: Vec<bae_core::collections::PlaylistEntry>| {
        entries.into_iter().map(crate::types::BridgePlaylistEntry::from_core).collect::<Vec<_>>()
    },
    built by playlist_entries
}

pulled_values! {
    /// The library's name.
    LibraryNameSubscription: String => String,
    converted by |name: String| name,
    built by library_name
}

#[cfg(feature = "desktop")]
pulled_values! {
    /// Where each watched folder's scan stands.
    FolderScansSubscription:
        bae_core::import::FolderScanProgress => crate::types::BridgeFolderScans,
    converted by crate::types::BridgeFolderScans::from_core,
    built by folder_scans
}
