//! The library as automation reads it: a release with its tracks, files
//! and gallery, the slim row a list shows, and what a search returns.
//!
//! Apart from the import types beside them: these describe what the
//! library already holds, not the folder being turned into a release.

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct AutomationRelease {
    pub summary: AutomationReleaseSummary,
    pub name: AutomationReleaseName,
    pub year: Option<i32>,
    pub labels: Vec<bae_types::pressing::ReleaseLabel>,
    pub facts: bae_types::pressing::PressingFacts,
    pub total_duration_ms: u64,
    pub tracks: Vec<AutomationTrackDetail>,
    pub track_groups: Vec<AutomationTrackGroup>,
    pub files: Vec<AutomationFileDetail>,
    pub image_files: Vec<AutomationFileDetail>,
    pub gallery_items: Vec<AutomationGalleryItem>,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<AutomationReleaseRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub media: Vec<bae_types::pressing::MediaCount>,
    /// Where its files are, and whether this device keeps them offline.
    pub storage_state: AutomationReleaseStorageState,
    /// The transitions available right now, derived by the core.
    pub storage_actions: Vec<AutomationReleaseStorageAction>,
    /// How many of the release's files wait in this device's upload queue,
    /// and how many of those are being sent now; `None` when none waits.
    pub upload: Option<AutomationReleaseUpload>,
    /// Whether an upload or a transfer is under way, which leaves
    /// `storage_actions` empty.
    pub transitioning: bool,
    pub file_count: i64,
    pub total_size: i64,
    pub cover: Option<AutomationImageRef>,
}

/// A release's files in this device's upload queue. Mirrors
/// `bae_views::album_detail::ReleaseUpload`'s counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AutomationReleaseUpload {
    pub queued: u32,
    pub uploading: u32,
}

/// A release's storage state. Mirrors `bae_views::album_detail::ReleaseStorageState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "place")]
pub enum AutomationReleaseStorageState {
    OnOneDevice,
    InStorage { pinned: bool },
}

impl From<ReleaseStorageState> for AutomationReleaseStorageState {
    fn from(state: ReleaseStorageState) -> Self {
        match state {
            ReleaseStorageState::OnOneDevice => Self::OnOneDevice,
            ReleaseStorageState::InStorage { pinned } => Self::InStorage { pinned },
        }
    }
}

/// A storage transition a release allows. Mirrors
/// `bae_types::release_storage::ReleaseStorageAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationReleaseStorageAction {
    Upload,
    Pin,
    Unpin,
    DetachAudio,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationImageRef {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrackDetail {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    pub artist_names: String,
    pub position_text: String,
    pub position: AutomationTrackPosition,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTrackPosition {
    Sided { side_letter: String, number: u32 },
    Disc { disc: u32, number: u32 },
    Flat { number: u32 },
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrackGroup {
    pub side: AutomationTrackSide,
    pub tracks: Vec<AutomationTrackDetail>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTrackSide {
    Sided { side_letter: String },
    Disc { disc: u32 },
    Flat,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationFileDetail {
    pub id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub is_image: bool,
    pub content_type: String,
    pub audio_format: Option<AutomationAudioFormat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationAudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationGalleryItem {
    pub id: String,
    pub label: String,
    pub source: AutomationGallerySource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationGallerySource {
    Cover { image: AutomationImageRef },
    ReleaseFile { file_id: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationLibrarySearchResults {
    pub albums: Vec<AutomationAlbumSearchResult>,
    pub tracks: Vec<AutomationTrackSearchResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_name: String,
    pub cover: Option<AutomationImageRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}
