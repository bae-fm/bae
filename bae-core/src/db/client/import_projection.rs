use super::identity::check_releases_in_library_on;
use super::import_state::load_import_candidate_states_on;
use super::payloads::load_all_source_release_payloads_on;
use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ImportTriageDbProjection {
    pub candidate_states: HashMap<String, DbImportCandidateState>,
    pub library_statuses: Vec<LibraryStatus>,
    pub source_payloads: HashMap<(crate::import::PayloadSource, String), String>,
}

impl Database {
    pub(crate) fn subscribe_import_triage(
        &self,
        snapshot: crate::import::ImportCandidatesSnapshot,
    ) -> coven::LiveQuery<ImportTriageDbProjection> {
        self.inner.handle.subscribe(move |sql| {
            let candidate_states =
                load_import_candidate_states_on(&sql).map_err(CovenError::from)?;
            let checks = crate::import::triage::library_checks(&snapshot, &candidate_states)
                .map_err(|error| CovenError::Database(DbError::Message(error.to_string())))?;
            let library_statuses =
                check_releases_in_library_on(&sql, &checks).map_err(CovenError::from)?;
            let source_payloads =
                load_all_source_release_payloads_on(&sql).map_err(CovenError::from)?;
            Ok(ImportTriageDbProjection {
                candidate_states,
                library_statuses,
                source_payloads,
            })
        })
    }
}
