//! The only place a screen subscription is built. Every value a screen shows
//! comes from a coven live query (core's typed wrappers over one) or from a
//! `KeyedValueStream`, and reaches the app by pull: the app awaits each
//! value's `next()`. Nothing here takes a channel or a closure loop.

use std::sync::Arc;

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
