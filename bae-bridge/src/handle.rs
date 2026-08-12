use bae_core::library::AppServices;
use bae_core::playback::QueueEntryId;
#[cfg(feature = "desktop")]
use bae_core::signals::ExtractionSource;
#[cfg(feature = "desktop")]
use bae_core::util::rate_limiter::CallPriority;
use tracing::info;

#[cfg(feature = "oauth-providers")]
use crate::types::BridgeCloudProvider;
#[cfg(feature = "desktop")]
use crate::types::BridgeDiscogsSaveOutcome;
#[cfg(any(feature = "cloudkit", feature = "oauth-providers"))]
use crate::types::BridgeHomeStorage;
#[cfg(feature = "desktop")]
use crate::types::BridgeRemoteCover;
use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeAlbumSearchResult, BridgeArtistDetail,
    BridgeArtistSortCriterion, BridgeArtistSummary, BridgeComposerDetail,
    BridgeComposerSortCriterion, BridgeComposerSummary, BridgeComposerWorkGroup, BridgeConfig,
    BridgeCoverSelection, BridgeError, BridgeFile, BridgeGalleryItem, BridgeGallerySource,
    BridgeMetadataSource, BridgePlaybackValues, BridgeQueueSnapshot, BridgeQueueUpcomingPage,
    BridgeRelease, BridgeReleaseRoleSummary, BridgeReleaseSummary, BridgeRepeatMode,
    BridgeSaveSyncConfig, BridgeSearchResults, BridgeSortCriterion, BridgeStorageFilter,
    BridgeStoragePage, BridgeStorageRow, BridgeStorageSort, BridgeSyncStatusSnapshot, BridgeTrack,
    BridgeTrackGroup, BridgeTrackRoleSummary, BridgeTrackSearchResult, BridgeWorkDetail,
    BridgeWorkReleaseSummary, BridgeWorkSummary, BridgeWorkTrackSummary,
};
#[cfg(feature = "desktop")]
use crate::types::{BridgeMcpServerStatus, BridgeStorageMode, BridgeSubsonicServerStatus};

#[derive(uniffi::Object)]
pub struct AppHandle {
    services: AppServices,
    ui_event_bus: bae_core::ui::UiEventBus,
    #[cfg(feature = "desktop")]
    desktop: bae_desktop::DesktopServices,
    #[cfg(feature = "cast")]
    cast: std::sync::Arc<bae_cast::CastController>,
    /// Last so every retained service is dropped while its tasks can still run.
    runtime: tokio::runtime::Runtime,
}

mod base;
mod desktop;
#[cfg(feature = "desktop")]
mod editing_projection;
mod import_projection;
mod library_projection;
mod playback_persistence;
mod queue_projection;
mod service_status;
use import_projection::convert_ui_event;
use queue_projection::pump_ui_events;

#[cfg(feature = "desktop")]
pub use editing_projection::{
    bridge_validation_reason_key, raw_release_edit_from_user_edit, shape_release_edit,
};

#[cfg(test)]
#[path = "handle_tests.rs"]
mod tests;
