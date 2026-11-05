#[macro_use]
extern crate bae_mirror;

use bae_app::AppServices;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::MetadataRef;
use bae_catalogs::remote_image::RemoteCover;
use bae_catalogs::search::MetadataResult;
use bae_catalogs::SearchQuery;
use bae_identify::release_group::ReleaseGroup;
use bae_identify::PressingLink;
use bae_import::folder_scanner::InvalidCandidate;
use bae_import::{
    CandidateEditField, CandidateImportStatus, CandidateKey, CandidateRuntimeSnapshot,
    CoverSelection, GroupedSearchResults, ImportCandidateDetail, ImportError, ImportInFlight,
    ImportListItem, ImportListView, ImportPhase, ImportStep, PrepareStep, TrackUserEdit, TriageTab,
};
use bae_library::LibraryError;
use bae_preferences::McpConfig;
use bae_records::GallerySource;
use bae_records::ImageRef;
use bae_records::LibraryStatus;
use bae_types::audio_format::AudioFormat;
use bae_types::mediums::{TrackPosition, TrackSide};
use bae_types::release_storage::ReleaseStorageAction;
use bae_views::album_detail::{
    FileDetail, GalleryItem, ReleaseDetail, ReleaseStorageState, SearchResults, TrackDetail,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

mod automation;
mod convert;
mod tool;
mod types;

use convert::*;
pub use tool::AutomationTool;
pub use types::*;

#[derive(Clone)]
pub struct Automation {
    services: AppServices,
}
