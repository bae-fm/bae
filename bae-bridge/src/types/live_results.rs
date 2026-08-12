use super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumPage {
    pub rows: Vec<BridgeAlbum>,
    pub total_count: u64,
}

#[uniffi::export(callback_interface)]
pub trait AlbumPageCallback: Send + Sync {
    fn on_value(&self, value: BridgeAlbumPage);
    fn on_error(&self, error: BridgeError);
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryParentObservation {
    pub child_count: u64,
}

#[uniffi::export(callback_interface)]
pub trait LibraryParentObservationCallback: Send + Sync {
    fn on_value(&self, value: BridgeLibraryParentObservation);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait AlbumDetailCallback: Send + Sync {
    fn on_value(&self, value: Option<BridgeAlbumDetail>);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait ReleaseDetailCallback: Send + Sync {
    fn on_value(&self, value: Option<BridgeRelease>);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait LibrarySearchCallback: Send + Sync {
    fn on_value(&self, value: BridgeSearchResults);
    fn on_error(&self, error: BridgeError);
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageProjection {
    pub page: BridgeStoragePage,
    pub total_size: u64,
}

#[uniffi::export(callback_interface)]
pub trait StorageProjectionCallback: Send + Sync {
    fn on_value(&self, value: BridgeStorageProjection);
    fn on_error(&self, error: BridgeError);
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistPage {
    pub rows: Vec<BridgeArtistSummary>,
    pub total_count: u64,
}

#[uniffi::export(callback_interface)]
pub trait ArtistPageCallback: Send + Sync {
    fn on_value(&self, value: BridgeArtistPage);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait ArtistDetailCallback: Send + Sync {
    fn on_value(&self, value: Option<BridgeArtistDetail>);
    fn on_error(&self, error: BridgeError);
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerPage {
    pub rows: Vec<BridgeComposerSummary>,
    pub total_count: u64,
}

#[uniffi::export(callback_interface)]
pub trait ComposerPageCallback: Send + Sync {
    fn on_value(&self, value: BridgeComposerPage);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait ComposerDetailCallback: Send + Sync {
    fn on_value(&self, value: Option<BridgeComposerDetail>);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait WorkDetailCallback: Send + Sync {
    fn on_value(&self, value: Option<BridgeWorkDetail>);
    fn on_error(&self, error: BridgeError);
}

#[cfg(feature = "cast")]
#[uniffi::export(callback_interface)]
pub trait CastDevicesCallback: Send + Sync {
    fn on_value(&self, devices: Vec<super::BridgeCastDevice>);
}

#[uniffi::export(callback_interface)]
pub trait ConfigCallback: Send + Sync {
    fn on_value(&self, config: BridgeConfig);
}

#[uniffi::export(callback_interface)]
pub trait SyncStatusCallback: Send + Sync {
    fn on_value(&self, value: BridgeSyncStatusSnapshot);
}

#[uniffi::export(callback_interface)]
pub trait QueueCallback: Send + Sync {
    fn on_value(&self, value: BridgeQueueSnapshot);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait QueueUpcomingCallback: Send + Sync {
    fn on_value(&self, value: BridgeQueueUpcomingPage);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait PlaybackValuesCallback: Send + Sync {
    fn on_value(&self, value: BridgePlaybackValues);
}

#[uniffi::export(callback_interface)]
pub trait OutboxCallback: Send + Sync {
    fn on_value(&self, value: BridgeOutboxSnapshot);
    fn on_error(&self, error: BridgeError);
}

#[uniffi::export(callback_interface)]
pub trait DownloadCallback: Send + Sync {
    fn on_value(&self, value: BridgeDownloadSnapshot);
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[uniffi::export(callback_interface)]
pub trait OutputCallback: Send + Sync {
    fn on_value(&self, value: BridgeOutputSnapshot);
}

#[cfg(feature = "desktop")]
#[uniffi::export(callback_interface)]
pub trait ImportCandidatesCallback: Send + Sync {
    fn on_value(&self, value: BridgeImportCandidatesSnapshot);
}

#[cfg(feature = "desktop")]
#[uniffi::export(callback_interface)]
pub trait ImportTriageCallback: Send + Sync {
    fn on_value(&self, value: BridgeTriageQueue);
    fn on_error(&self, error: BridgeError);
}

#[cfg(feature = "desktop")]
#[uniffi::export(callback_interface)]
pub trait ReleaseLibraryStatusCallback: Send + Sync {
    fn on_value(&self, value: BridgeLibraryStatus);
    fn on_error(&self, error: BridgeError);
}
