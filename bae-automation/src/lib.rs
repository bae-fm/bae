#[macro_use]
extern crate bae_mirror;

use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::RemoteCover;
use bae_core::album_detail::{
    AudioFormat, FileDetail, GalleryItem, GallerySource, ImageRef, ReleaseDetail,
    ReleaseStorageAction, ReleaseStorageState, SearchResults, TrackDetail, TrackPosition,
    TrackSide,
};
use bae_core::config::McpConfig;
use bae_core::db::LibraryStatus;
use bae_core::import::folder_scanner::InvalidCandidate;
use bae_core::import::release_group::ReleaseGroup;
use bae_core::import::search::MetadataResult;
use bae_core::import::{
    CandidateEditField, CandidateImportStatus, CandidateKey, CandidateRuntimeSnapshot,
    CoverSelection, GroupedSearchResults, ImportCandidateDetail, ImportError, ImportInFlight,
    ImportListItem, ImportListView, ImportPhase, ImportStep, MetadataRef, PrepareStep,
    PressingLink, SearchQuery, TrackUserEdit, TriageTab,
};
use bae_core::library::{AppServices, LibraryError};
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
