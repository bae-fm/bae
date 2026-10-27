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
    Request,
    Write,
    RequestAndWrite,
}

callbacks! {
    QueueCallback: on_value(value: BridgeQueueSnapshot) + on_error;
    UploadsPausedCallback: on_value(paused: bool) + on_error;
    /// Every upload rate and the total as a `Reset` when the subscription
    /// opens, then each batch of the ones that moved or left, in one call.
    OutboxTransfersCallback: on_changes(changes: Vec<super::BridgeOutboxTransferChange>) + on_error;
    /// The files being sent together when the subscription opens, then each
    /// time they move.
    OutboxTotalTransferCallback: on_value(transfer: super::BridgeUploadTransfer) + on_error;

    #[cfg(feature = "cast")]
    CastDevicesCallback: on_value(devices: Vec<super::BridgeCastDevice>);
    ConfigCallback: on_value(config: BridgeConfig);
    SyncStatusCallback: on_value(value: BridgeSyncStatusSnapshot);
    SyncActivityCallback: on_value(value: super::BridgeSyncActivity) + on_error;
    PlaybackValuesCallback: on_value(value: BridgePlaybackValues) + on_error;
    PlaybackPositionsCallback: on_value(value: BridgePlaybackPositions);
    DownloadCountsCallback: on_value(value: super::BridgeDownloadCounts) + on_error;
    /// Every downloading release's progress as a `Reset` when the
    /// subscription opens, then each batch of the ones that moved or ended,
    /// in one call.
    DownloadTransfersCallback: on_changes(changes: Vec<super::BridgeDownloadTransferChange>);
    /// Every exporting release's percent as a `Reset` when the subscription
    /// opens, then each batch of the ones that moved or ended, in one call.
    #[cfg(desktop)]
    OutputTransfersCallback: on_changes(changes: Vec<super::BridgeOutputTransferChange>);

    /// What every candidate has in flight: a `Reset` with every running key
    /// when the subscription opens, then `Updated` as a key advances and
    /// `Removed` once nothing is running for it, each batch in one call.
    #[cfg(feature = "desktop")]
    CandidateRuntimeCallback: on_changes(changes: Vec<BridgeCandidateRuntimeChange>);
    /// Where each watched folder's scan stands, now and as it changes.
    #[cfg(feature = "desktop")]
    FolderScansCallback: on_value(value: BridgeFolderScans) + on_error;
    /// How far a bulk action over the selection has got.
    #[cfg(feature = "desktop")]
    SelectionActionProgressCallback: on_progress(progress: BridgeSelectionActionProgress);
}
