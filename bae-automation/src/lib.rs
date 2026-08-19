#![deny(unreachable_pub, dead_code)]

use bae_core::album_detail::{
    AudioFormat, FileDetail, GalleryItem, GallerySource, ImageRef, ReleaseDetail,
    ReleaseStorageAction, ReleaseStorageState, SearchResults, TrackDetail, TrackPosition,
    TrackSide,
};
use bae_core::config::McpConfig;
use bae_core::db::LibraryStatus;
use bae_core::import::cover_art::RemoteCover;
use bae_core::import::folder_scanner::{FolderCandidate, InvalidCandidate};
use bae_core::import::release_group::ReleaseGroup;
use bae_core::import::search::{ImportSearchReleaseDetail, MetadataResult};
use bae_core::import::{
    shape_user_edit_for_choice, CoverSelection, GroupedSearchResults, IdentityChoice, ImportError,
    ImportEvent, ImportPhase, ImportProgress, MetadataRef, MetadataSource, PrepareStep,
    PressingEdit, ScanEvent, SearchQuery, StorageMode, TrackUserEdit,
};
use bae_core::library::{AppServices, LibraryError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::warn;

mod automation;
mod convert;
mod tool;
mod types;

use convert::*;
pub use tool::AutomationTool;
pub use types::*;

struct AutomationState {
    candidates: RwLock<HashMap<String, AutomationCandidate>>,
    event_indexing: RwLock<AutomationEventIndexing>,
}

impl AutomationState {
    fn new() -> Self {
        Self {
            candidates: RwLock::new(HashMap::new()),
            event_indexing: RwLock::new(AutomationEventIndexing::NotStarted),
        }
    }

    fn start_event_indexing(&self) -> bool {
        let mut event_indexing = self
            .event_indexing
            .write()
            .expect("event indexing state poisoned");
        match *event_indexing {
            AutomationEventIndexing::NotStarted => {
                *event_indexing = AutomationEventIndexing::Started;
                true
            }
            AutomationEventIndexing::Started | AutomationEventIndexing::Failed { .. } => false,
        }
    }

    fn apply_event(&self, event: ImportEvent) {
        match event {
            ImportEvent::Scan(event) => self.apply_scan_event(event),
            ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } => self.update_candidate(candidate_key, |candidate| {
                candidate.common_mut().runtime_mut().progress =
                    Some(automation_import_progress(progress));
            }),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            ImportEvent::ImportLoudnessProgress { .. } => {}
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                toolbar,
                // The automation view mirrors every run, whoever started it.
                priority: _,
            } => self.update_candidate(candidate_key, |candidate| {
                let common = candidate.common_mut();
                let runtime = common.runtime_mut();
                runtime.identify_state = Some(automation_identify_state(state));
                runtime.toolbar =
                    Some(toolbar.into_iter().map(automation_toolbar_signal).collect());
            }),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            ImportEvent::SignalsUpdated {
                candidate_key,
                signals,
                priority: _,
            } => self.update_candidate(candidate_key, |candidate| {
                candidate.common_mut().runtime_mut().signals = Some(automation_signals(signals));
            }),
            // A queue-wide count with no candidate to attach it to. The
            // automation surface is per-candidate, so there is nothing here to
            // update.
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            ImportEvent::QueueIdentifyProgress { .. } => {}
        }
    }

    fn apply_scan_event(&self, event: ScanEvent) {
        match event {
            ScanEvent::WatchedFoldersChanged { .. } => {}
            // A binding change re-derives the candidate's track count and
            // format label, so it replaces the indexed row exactly as a fresh
            // scan of the folder would.
            ScanEvent::CandidateDiscovered {
                candidate,
                skipped,
                is_added,
            }
            | ScanEvent::FolderCandidate {
                candidate,
                skipped,
                is_added,
            } => {
                self.insert_candidate(automation_candidate_from_folder(
                    candidate, skipped, is_added,
                ));
            }
            ScanEvent::CandidateBindingChanged { candidate } => {
                let key = candidate.path.to_string_lossy().into_owned();
                let status = self
                    .candidates
                    .read()
                    .expect("candidate index poisoned")
                    .get(&key)
                    .map(|candidate| (candidate.common().skipped, candidate.common().is_added));
                if let Some((skipped, is_added)) = status {
                    self.insert_candidate(automation_candidate_from_folder(
                        candidate, skipped, is_added,
                    ));
                } else {
                    self.fail_missing_candidate("binding change", &key);
                }
            }
            ScanEvent::FolderReleaseBoundary(_) => {}
            ScanEvent::InvalidCandidate(candidate) => {
                self.insert_candidate(automation_candidate_from_invalid(candidate));
            }
            ScanEvent::CandidateRemoved { candidate_key } => {
                let removed = self
                    .candidates
                    .write()
                    .expect("candidate index poisoned")
                    .remove(&candidate_key);
                if removed.is_none() {
                    self.fail_missing_candidate("removal", &candidate_key);
                }
            }
            ScanEvent::CandidateSkipChanged {
                candidate_key,
                skipped,
            } => self.update_candidate(candidate_key, |candidate| {
                candidate.common_mut().skipped = skipped;
            }),
            ScanEvent::FolderScanStatusChanged { status } => {
                if let bae_core::import::FolderScanStatus::Failed { error } = status.status {
                    self.fail_event_indexing(error);
                }
            }
            ScanEvent::CandidateVerdictStored { .. }
            | ScanEvent::CandidateIdentityPicked { .. }
            | ScanEvent::Finished => {}
        }
    }

    fn insert_candidate(&self, candidate: AutomationCandidate) {
        self.candidates
            .write()
            .expect("candidate index poisoned")
            .insert(candidate.key().to_string(), candidate);
    }

    /// Seed the index from the import service's current candidate snapshot.
    ///
    /// The scanner runs from bootstrap; event indexing starts with the
    /// automation surface. An index built from events alone does not know any
    /// candidate discovered in between, so that candidate's first update
    /// latched the whole surface `Failed`. Seeding makes the index start from
    /// the state it mirrors; an unknown key after the seed remains a loud
    /// failure, because then it is a real contradiction. Called after the
    /// event subscription exists — an event raced in between re-applies over
    /// the seed, which is idempotent for inserts and ordinary for updates.
    fn seed_candidates(&self, snapshot: bae_core::import::ImportCandidatesSnapshot) {
        for folder in snapshot.folder_candidates {
            self.insert_candidate(automation_candidate_from_folder(
                folder.candidate,
                folder.skipped,
                folder.is_added,
            ));
        }
        for invalid in snapshot.invalid_candidates {
            self.insert_candidate(automation_candidate_from_invalid(invalid));
        }
    }

    fn update_candidate(
        &self,
        candidate_key: String,
        update: impl FnOnce(&mut AutomationCandidate),
    ) {
        let updated = {
            let mut candidates = self.candidates.write().expect("candidate index poisoned");
            if let Some(candidate) = candidates.get_mut(&candidate_key) {
                update(candidate);
                true
            } else {
                false
            }
        };
        if !updated {
            self.fail_missing_candidate("update", &candidate_key);
        }
    }

    fn fail_missing_candidate(&self, action: &str, candidate_key: &str) {
        let message =
            format!("automation candidate {action} referenced unknown candidate '{candidate_key}'");
        warn!("{message}");
        self.fail_event_indexing(message);
    }

    fn fail_event_indexing(&self, message: String) {
        let mut event_indexing = self
            .event_indexing
            .write()
            .expect("event indexing state poisoned");
        *event_indexing = AutomationEventIndexing::Failed { message };
    }

    fn event_indexing(&self) -> AutomationEventIndexing {
        self.event_indexing
            .read()
            .expect("event indexing state poisoned")
            .clone()
    }

    fn ensure_event_index_ready(&self) -> Result<(), AutomationError> {
        let event_indexing = self
            .event_indexing
            .read()
            .expect("event indexing state poisoned");
        match &*event_indexing {
            AutomationEventIndexing::Failed { message } => {
                Err(AutomationError::Unavailable(message.clone()))
            }
            AutomationEventIndexing::Started | AutomationEventIndexing::NotStarted => Ok(()),
        }
    }

    fn candidate_count(&self) -> usize {
        self.candidates
            .read()
            .expect("candidate index poisoned")
            .len()
    }

    /// One candidate by key, or `NotFound`. A key the index has never seen
    /// names nothing — distinct from a candidate that exists but whose identify
    /// pipeline hasn't run, which is a real candidate with no evidence yet.
    fn get_candidate(&self, candidate_key: &str) -> Result<AutomationCandidate, AutomationError> {
        self.ensure_event_index_ready()?;
        self.candidates
            .read()
            .expect("candidate index poisoned")
            .get(candidate_key)
            .cloned()
            .ok_or_else(|| {
                AutomationError::not_found(format!("candidate '{candidate_key}' not found"))
            })
    }

    fn list_candidates(&self) -> Result<Vec<AutomationCandidate>, AutomationError> {
        self.ensure_event_index_ready()?;
        let mut candidates = self
            .candidates
            .read()
            .expect("candidate index poisoned")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.path().cmp(b.path()));
        Ok(candidates)
    }
}

#[derive(Clone)]
pub struct Automation {
    services: AppServices,
    runtime_handle: tokio::runtime::Handle,
    state: Arc<AutomationState>,
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
