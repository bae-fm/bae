#![deny(unreachable_pub, dead_code)]

use bae_core::album_detail::{
    AudioFormat, FileDetail, GalleryItem, GallerySource, ImageRef, ReleaseDetail,
    ReleaseStorageAction, ReleaseStorageState, SearchResults, TrackDetail, TrackPosition,
    TrackSide,
};
use bae_core::config::McpConfig;
use bae_core::db::LibraryStatus;
use bae_core::import::cover_art::RemoteCover;
use bae_core::import::folder_scanner::InvalidCandidate;
use bae_core::import::release_group::ReleaseGroup;
use bae_core::import::search::{ImportSearchReleaseDetail, MetadataResult};
use bae_core::import::{
    shape_user_edit_for_choice, CandidateImportStatusSnapshot, CandidateRuntimeSnapshot,
    CoverSelection, FolderImportCandidateSnapshot, GroupedSearchResults, IdentityChoice,
    ImportCandidatesSnapshot, ImportError, ImportPhase, ImportStep, MetadataRef, MetadataSource,
    PrepareStep, PressingEdit, ScanEvent, SearchQuery, StorageMode, TrackUserEdit,
};
use bae_core::library::{AppServices, LibraryError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

mod automation;
mod convert;
mod tool;
mod types;

use convert::*;
pub use tool::AutomationTool;
pub use types::*;

/// The import service's candidate state, converted on read.
///
/// The automation surface mirrors state the import service already owns and
/// publishes: `subscribe_import_candidates` is a watch whose value is the
/// current [`ImportCandidatesSnapshot`]. Holding that receiver and converting
/// on read means there is no second copy to drift out of step, no event order
/// to reconstruct, and no such thing as an update naming a candidate this
/// surface has not seen.
///
/// That last one is why the mirror is read rather than built: an index fed by
/// `ImportEvent` had to decide what an update for an unknown key meant, and
/// every class of key it had not indexed — candidates discovered before it
/// subscribed, paths a `FolderReleaseBoundary` hides, `reidentify:` runs that
/// name no candidate at all — latched the whole surface dead. A watch has no
/// unknown-key concept: the latest value is the answer.
struct AutomationState {
    candidates: watch::Receiver<ImportCandidatesSnapshot>,
}

impl AutomationState {
    fn new(candidates: watch::Receiver<ImportCandidatesSnapshot>) -> Self {
        Self { candidates }
    }

    fn watched_folders(&self) -> Vec<bae_core::import::WatchedFolder> {
        self.candidates.borrow().watched_folders.clone()
    }

    fn candidate_count(&self) -> usize {
        let snapshot = self.candidates.borrow();
        snapshot.folder_candidates.len() + snapshot.invalid_candidates.len()
    }

    fn list_candidates(&self) -> Vec<AutomationCandidate> {
        automation_candidates(&self.candidates.borrow())
    }

    /// One candidate by key, or `NotFound`. A request naming a key the import
    /// service is not publishing is refused rather than answered: core reads a
    /// key it has recorded nothing against as "the pipeline hasn't run", which
    /// is right for a scanned candidate awaiting identification and
    /// indistinguishable from a typo.
    fn get_candidate(&self, candidate_key: &str) -> Result<AutomationCandidate, AutomationError> {
        automation_candidate(&self.candidates.borrow(), candidate_key).ok_or_else(|| {
            AutomationError::not_found(format!("candidate '{candidate_key}' not found"))
        })
    }
}

#[derive(Clone)]
pub struct Automation {
    services: AppServices,
    state: Arc<AutomationState>,
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
