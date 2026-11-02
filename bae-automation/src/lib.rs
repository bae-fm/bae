#[macro_use]
extern crate bae_mirror;

use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::remote_image::RemoteCover;
use bae_core::album_detail::{
    FileDetail, GalleryItem, GallerySource, ReleaseDetail, ReleaseStorageState, SearchResults,
    TrackDetail,
};
use bae_core::catalog_model::records::MetadataRef;
use bae_core::catalog_model::search::MetadataResult;
use bae_core::catalog_model::SearchQuery;
use bae_core::config::McpConfig;
use bae_core::db::ImageRef;
use bae_core::db::LibraryStatus;
use bae_core::identify::release_group::ReleaseGroup;
use bae_core::identify::PressingLink;
use bae_core::import::folder_scanner::InvalidCandidate;
use bae_core::import::{
    CandidateEditField, CandidateImportStatus, CandidateKey, CandidateRuntimeSnapshot,
    CoverSelection, GroupedSearchResults, ImportCandidateDetail, ImportError, ImportInFlight,
    ImportListItem, ImportListView, ImportPhase, ImportStep, PrepareStep, TrackUserEdit, TriageTab,
};
use bae_core::library::{AppServices, LibraryError};
use bae_types::audio_format::AudioFormat;
use bae_types::mediums::{TrackPosition, TrackSide};
use bae_types::release_storage::ReleaseStorageAction;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;
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

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
