//! The only place a screen subscription is built. Every value a screen shows
//! comes from a coven live query (core's typed wrappers over one) or from a
//! `KeyedValueStream`, and reaches the app by pull: the app awaits each
//! value's `next()`. Nothing here takes a channel or a closure loop.

use std::sync::Arc;

use crate::keyed_values::{KeyedValueStream, StreamValues};
use crate::types::BridgeError;

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

/// A setup call's steps, or an open's, as the app reads them: the stream's
/// one key, `""`, while the call runs a step. The call that is handed the
/// stream's values writes them; cancelling the stream ends what it holds.
pub(crate) fn setup_steps(values: Arc<StreamValues>) -> Arc<KeyedValueStream> {
    Arc::new(KeyedValueStream::new(
        values,
        bae_core::library::CancellationToken::new(),
    ))
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

/// Whether the releases an import pane offers are already in the library:
/// one live query for as long as the pane is open, whose checks move in place
/// as its offers change. It starts with no checks.
#[cfg(feature = "desktop")]
#[derive(uniffi::Object)]
pub struct LibraryStatusSubscription {
    inner: bae_core::library::LibraryStatusSubscription,
    runtime: crate::operation_runtime::OperationRuntime,
}

#[cfg(feature = "desktop")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl LibraryStatusSubscription {
    /// Check `checks` from now on. Returns the revision the answer will
    /// carry.
    pub fn set_checks(
        &self,
        checks: Vec<crate::handle::BridgeLibraryCheck>,
    ) -> Result<u64, BridgeError> {
        self.inner
            .set_checks(checks.into_iter().map(|check| bae_core::db::LibraryCheck {
                release_id: check.release_id,
                source: check.source.into_core(),
                source_group_id: check.source_group_id,
            }))
            .map_err(status_error)
    }

    pub async fn next(
        self: Arc<Self>,
    ) -> Result<crate::handle::BridgeLibraryStatusSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("LibraryStatusSubscription.next", move || async move {
                self.inner
                    .next()
                    .await
                    .map(|snapshot| crate::handle::BridgeLibraryStatusSnapshot {
                        statuses: snapshot
                            .statuses
                            .into_iter()
                            .map(|status| {
                                (
                                    status.release_id.clone(),
                                    crate::types::BridgeLibraryStatus::from_core(status),
                                )
                            })
                            .collect(),
                        request_revision: snapshot.request_revision,
                    })
                    .map_err(status_error)
            })
            .await
    }

    pub async fn cancel(self: Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("LibraryStatusSubscription.cancel", move || async move {
                self.inner.cancel().await;
                Ok(())
            })
            .await
    }
}

#[cfg(feature = "desktop")]
fn status_error(error: bae_core::library::LibraryStatusSubscriptionError) -> BridgeError {
    match error {
        bae_core::library::LibraryStatusSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LibraryStatusSubscriptionError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn library_statuses(
    inner: bae_core::library::LibraryStatusSubscription,
    runtime: crate::operation_runtime::OperationRuntime,
) -> Arc<LibraryStatusSubscription> {
    Arc::new(LibraryStatusSubscription { inner, runtime })
}
