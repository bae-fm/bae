//! The library as automation reads it: a release with its tracks, files
//! and gallery, the slim row a list shows, and what a search returns.
//!
//! Apart from the import types beside them: these describe what the
//! library already holds, not the folder being turned into a release.

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct AutomationRelease {
    pub summary: AutomationReleaseSummary,
    pub display_name: String,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub total_duration_ms: i64,
    pub tracks: Vec<AutomationTrackDetail>,
    pub track_groups: Vec<AutomationTrackGroup>,
    pub files: Vec<AutomationFileDetail>,
    pub image_files: Vec<AutomationFileDetail>,
    pub gallery_items: Vec<AutomationGalleryItem>,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<AutomationReleaseRecord>,
    /// Every name read off the object itself, one line per value and in the
    /// order surfaces list mark kinds. Empty when its folder stated none.
    pub marks: Vec<AutomationReleaseMark>,
    /// What the rip databases said about this release's audio. `None` for a
    /// release no source verified.
    pub verification: Option<AutomationVerification>,
    /// Which name read off the object tied its files to the record its draft
    /// was read from. `None` where nothing did.
    pub identified_by: Option<AutomationMarkKind>,
    /// Whether other copies of this release's audio agree with it — every
    /// track confirmed by at least one database.
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub format: Option<String>,
    /// Where the audio lives. Orthogonal to `pinned`.
    pub storage_state: AutomationReleaseStorageState,
    /// Whether a Remote release is kept offline on this device — the orthogonal
    /// cache property, never folded into `storage_state`.
    pub pinned: bool,
    /// The transitions available right now, derived by the core.
    pub storage_actions: Vec<AutomationReleaseStorageAction>,
    /// The transition currently in flight, if any — so a client can tell a release
    /// is mid-transfer rather than reading `storage_actions` and guessing.
    pub transfer_action: Option<AutomationReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
    pub cover: Option<AutomationImageRef>,
}

/// A release's storage state. Mirrors `bae_core::album_detail::ReleaseStorageState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationReleaseStorageState {
    Local,
    Remote,
}

impl From<ReleaseStorageState> for AutomationReleaseStorageState {
    fn from(state: ReleaseStorageState) -> Self {
        match state {
            ReleaseStorageState::Local => Self::Local,
            ReleaseStorageState::Remote => Self::Remote,
        }
    }
}

/// A storage transition a release allows. Mirrors
/// `bae_core::album_detail::ReleaseStorageAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationReleaseStorageAction {
    MakeRemote,
    Pin,
    Unpin,
    MakeLocal,
}

impl From<ReleaseStorageAction> for AutomationReleaseStorageAction {
    fn from(action: ReleaseStorageAction) -> Self {
        match action {
            ReleaseStorageAction::MakeRemote => Self::MakeRemote,
            ReleaseStorageAction::Pin => Self::Pin,
            ReleaseStorageAction::Unpin => Self::Unpin,
            ReleaseStorageAction::MakeLocal => Self::MakeLocal,
        }
    }
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
    pub side: i32,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    pub artist_names: String,
    pub position_text: String,
    pub position: AutomationTrackPosition,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTrackPosition {
    Sided { side_letter: String, number: i32 },
    SidedUnnumbered { side_letter: String },
    Disc { disc: i32, number: i32 },
    DiscUnnumbered { disc: i32 },
    Flat { number: i32 },
    Unnumbered,
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
    Disc { disc: i32 },
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
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}
