use super::identity::check_releases_in_library_on;
use super::import_content_hash::imported_releases_for_content_hashes_on;
use super::import_state::load_import_candidate_states_on;
use super::payloads::load_all_source_release_payloads_on;
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImportTriageDbProjection {
    pub candidate_states: HashMap<String, DbImportCandidateState>,
    pub library_statuses: Vec<LibraryStatus>,
    pub source_payloads: HashMap<(crate::import::PayloadSource, String), String>,
    pub imported_releases: HashMap<String, crate::import::ImportedRelease>,
}

impl Database {
    pub(crate) fn subscribe_import_triage(
        &self,
        snapshot: crate::import::ImportCandidatesSnapshot,
    ) -> coven::LiveQuery<ImportTriageDbProjection> {
        self.inner.handle.subscribe(move |sql| {
            let mut content_hashes: Vec<_> = snapshot
                .folder_candidates
                .iter()
                .map(|candidate| candidate.candidate.files.content_hash())
                .collect();
            content_hashes.sort();
            content_hashes.dedup();
            let candidate_states =
                load_import_candidate_states_on(&sql).map_err(CovenError::from)?;
            let checks = crate::import::triage::library_checks(&snapshot, &candidate_states)
                .map_err(|error| {
                    CovenError::Database(Box::new(DbError::Message(error.to_string())))
                })?;
            let library_statuses =
                check_releases_in_library_on(&sql, &checks).map_err(CovenError::from)?;
            let source_payloads =
                load_all_source_release_payloads_on(&sql).map_err(CovenError::from)?;
            let imported_releases = imported_releases_for_content_hashes_on(&sql, &content_hashes)
                .map_err(CovenError::from)?;
            Ok(ImportTriageDbProjection {
                candidate_states,
                library_statuses,
                source_payloads,
                imported_releases,
            })
        })
    }
}
