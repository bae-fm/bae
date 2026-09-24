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

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumPage {
    pub rows: Vec<BridgeAlbum>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLibraryPageWindow {
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLiveQueryCause {
    Initial,
    RequestChanged,
    DatabaseChanged,
    RequestAndDatabaseChanged,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageProjection {
    pub page: BridgeStoragePage,
    pub total_size: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistPage {
    pub rows: Vec<BridgeArtistSummary>,
    pub total_count: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerPage {
    pub rows: Vec<BridgeComposerSummary>,
    pub total_count: u64,
}

callbacks! {
    AlbumPageCallback: on_value(value: BridgeAlbumPage) + on_error;
    AlbumDetailCallback: on_value(value: Option<BridgeAlbumDetail>) + on_error;
    ReleaseDetailCallback: on_value(value: Option<BridgeRelease>) + on_error;
    LibrarySearchCallback: on_value(value: BridgeSearchResults) + on_error;
    StorageProjectionCallback: on_value(value: BridgeStorageProjection) + on_error;
    ArtistPageCallback: on_value(value: BridgeArtistPage) + on_error;
    ArtistDetailCallback: on_value(value: Option<BridgeArtistDetail>) + on_error;
    ComposerPageCallback: on_value(value: BridgeComposerPage) + on_error;
    ComposerDetailCallback: on_value(value: Option<BridgeComposerDetail>) + on_error;
    WorkDetailCallback: on_value(value: Option<BridgeWorkDetail>) + on_error;
    QueueCallback: on_value(value: BridgeQueueSnapshot) + on_error;
    QueueUpcomingCallback: on_value(value: BridgeQueueUpcomingPage) + on_error;
    OutboxCallback: on_value(value: BridgeOutboxSnapshot) + on_error;

    #[cfg(feature = "cast")]
    CastDevicesCallback: on_value(devices: Vec<super::BridgeCastDevice>);
    ConfigCallback: on_value(config: BridgeConfig);
    SyncStatusCallback: on_value(value: BridgeSyncStatusSnapshot);
    EagerCacheFillStatusCallback: on_value(value: BridgeEagerCacheFillStatus);
    PlaybackValuesCallback: on_value(value: BridgePlaybackValues);
    DownloadCallback: on_value(value: BridgeDownloadSnapshot);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    OutputCallback: on_value(value: BridgeOutputSnapshot);

    /// One candidate as the pane reads it. `None` once the key names no scanned
    /// folder, which is what clears a selection.
    #[cfg(feature = "desktop")]
    ImportCandidateCallback: on_value(value: Option<BridgeImportCandidateDetail>) + on_error;
    /// What every candidate has in flight: one `Updated` per key already running
    /// when the subscription opens, then one call per change — `Updated` as a key
    /// advances, `Removed` once nothing is running for it, and `Reset` carrying
    /// every running key after a dropped delivery, which a consumer holding a key
    /// the reset does not list reads as that key's removal.
    #[cfg(feature = "desktop")]
    CandidateRuntimeCallback: on_change(change: BridgeCandidateRuntimeChange);
    #[cfg(feature = "desktop")]
    ReleaseLibraryStatusCallback: on_value(value: BridgeLibraryStatus) + on_error;
}
