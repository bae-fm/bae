use super::folder_scans::load_folder_scan_snapshots_on;
use super::identity::check_releases_in_library_on;
use super::import_content_hash::{
    imported_content_hashes_on, imported_releases_for_content_hashes_on,
};
use super::import_state::load_import_candidate_states_on;
use super::payloads::load_all_source_release_payloads_on;
use super::*;

/// The import tab, read in one snapshot: the candidate list and everything
/// the triage rows derive from stored state.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidatesProjection {
    pub snapshot: crate::import::ImportCandidatesSnapshot,
    pub triage: ImportTriageDbProjection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportTriageDbProjection {
    pub candidate_states: HashMap<String, DbImportCandidateState>,
    pub library_statuses: Vec<LibraryStatus>,
    pub source_payloads: HashMap<(crate::import::PayloadSource, String), String>,
    pub imported_releases: HashMap<String, crate::import::ImportedRelease>,
}

fn load_import_candidates_on(
    sql: &SqlReadContext<'_>,
) -> Result<ImportCandidatesProjection, DbError> {
    let watched_folders = sql
        .query(
            "SELECT path FROM watched_import_folders ORDER BY position",
            [],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .map(crate::import::WatchedFolder::from_path)
        .collect();
    let skipped: HashSet<(String, String)> = sql
        .query(
            "SELECT watched_folder_path, relative_candidate_path FROM skipped_import_candidates",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .into_iter()
        .collect();
    let scans = load_folder_scan_snapshots_on(sql)?;
    let imported_content_hashes = imported_content_hashes_on(sql)?;
    let mut snapshot = crate::import::candidates::build_snapshot(
        watched_folders,
        scans,
        &skipped,
        &imported_content_hashes,
    )
    .map_err(|error| DbError::Message(error.to_string()))?;

    let mut content_hashes: Vec<_> = snapshot
        .folder_candidates
        .iter()
        .map(|candidate| candidate.candidate.files.content_hash())
        .collect();
    content_hashes.sort();
    content_hashes.dedup();
    let candidate_states = load_import_candidate_states_on(sql)?;
    let checks = crate::import::triage::library_checks(&snapshot, &candidate_states)
        .map_err(|error| DbError::Message(error.to_string()))?;
    let library_statuses = check_releases_in_library_on(sql, &checks)?;
    let source_payloads = load_all_source_release_payloads_on(sql)?;
    let imported_releases = imported_releases_for_content_hashes_on(sql, &content_hashes)?;
    crate::import::triage::resume_stored_verdicts(
        &mut snapshot,
        &candidate_states,
        &library_statuses,
    )
    .map_err(|error| DbError::Message(error.to_string()))?;
    Ok(ImportCandidatesProjection {
        snapshot,
        triage: ImportTriageDbProjection {
            candidate_states,
            library_statuses,
            source_payloads,
            imported_releases,
        },
    })
}

impl Database {
    /// The import tab as a live query: the initial read, then one value per
    /// commit that can change it — a scan item, a skip, a verdict, a pick, an
    /// import landing — with unchanged reruns withheld by coven.
    pub(crate) fn subscribe_import_candidates(
        &self,
    ) -> coven::LiveQuery<ImportCandidatesProjection> {
        self.inner
            .handle
            .subscribe(move |sql| load_import_candidates_on(&sql).map_err(CovenError::from))
    }

    pub(crate) async fn load_import_candidates(
        &self,
    ) -> Result<ImportCandidatesProjection, DbError> {
        self.read(move |sql| load_import_candidates_on(&sql)).await
    }
}
