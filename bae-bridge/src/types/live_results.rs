use super::*;

/// The callback interfaces a host implements to receive a subscription's
/// values.
///
/// A subscription either can fail — a live query over the database — or cannot,
/// and that is the whole difference between the two shapes: one carries
/// `on_error`, the other does not. Each trait is named and its method's
/// parameter is named, because those names are what the generated Swift, Kotlin
/// and C# read.
macro_rules! callbacks {
    (
        $(
            $(#[$attr:meta])*
            $name:ident: $method:ident($param:ident: $value:ty)
            $(+ $on_error:ident)? ;
        )*
    ) => {
        $(
            $(#[$attr])*
            #[uniffi::export(callback_interface)]
            pub trait $name: Send + Sync {
                fn $method(&self, $param: $value);
                $( fn $on_error(&self, error: BridgeError); )?
            }
        )*
    };
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLibraryPageWindow {
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLibraryBrowseSection {
    pub id: String,
    pub title: String,
    pub window: BridgeLibraryPageWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLiveQueryCause {
    Initial,
    RequestChanged,
    DatabaseChanged,
    RequestAndDatabaseChanged,
}

callbacks! {
    QueueCallback: on_value(value: BridgeQueueSnapshot) + on_error;
    OutboxCallback: on_value(value: BridgeOutboxSnapshot) + on_error;

    #[cfg(feature = "cast")]
    CastDevicesCallback: on_value(devices: Vec<super::BridgeCastDevice>);
    ConfigCallback: on_value(config: BridgeConfig);
    SyncStatusCallback: on_value(value: BridgeSyncStatusSnapshot);
    EagerCacheFillStatusCallback: on_value(value: BridgeEagerCacheFillStatus);
    PlaybackValuesCallback: on_value(value: BridgePlaybackValues) + on_error;
    DownloadCallback: on_value(value: BridgeDownloadSnapshot) + on_error;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    OutputCallback: on_value(value: BridgeOutputSnapshot) + on_error;

    /// What every candidate has in flight: a `Reset` with every running key
    /// when the subscription opens, then `Updated` as a key advances and
    /// `Removed` once nothing is running for it.
    #[cfg(feature = "desktop")]
    CandidateRuntimeCallback: on_change(change: BridgeCandidateRuntimeChange);
    /// What the import list's selection holds and can be told to do: the
    /// value on opening, then one call each time it changes.
    #[cfg(feature = "desktop")]
    ImportSelectionCallback: on_value(value: BridgeSelectionSummary);
    /// How far a bulk action over the selection has got.
    #[cfg(feature = "desktop")]
    SelectionActionProgressCallback: on_progress(progress: BridgeSelectionActionProgress);
}
