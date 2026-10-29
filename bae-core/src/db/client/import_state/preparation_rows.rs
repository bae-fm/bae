//! One candidate's whole stored state, loaded and saved as a unit.
//!
//! Every row group under a content hash — the candidate row, its verdict with
//! the matches beneath it, its signals, its draft with its provenance, cover,
//! and prepared assets, its file decisions — is read into one
//! [`CandidatePreparation`] and written back from one, in one transaction
//! guarded by the revisions the caller loaded. The rules about what a
//! candidate may hold live on the value, not here.

use super::*;
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::folder_scanner::FolderCandidate;
use crate::import::preparation::CandidatePreparation;

/// The revisions a save was prepared against, and — for a write that must
/// not land on a folder the scan has since re-read — where the scan lists it.
#[derive(Debug, Clone)]
pub(crate) struct CandidateSaveExpectation {
    pub edit_revision: u64,
    pub metadata_revision: u64,
    pub scanned: Option<ScannedCandidateKey>,
}

/// Where the scan tables list a candidate: the root it is under and its key.
#[derive(Debug, Clone)]
pub(crate) struct ScannedCandidateKey {
    pub watched_folder_path: String,
    pub candidate_path: String,
}

impl ScannedCandidateKey {
    /// Refuse the write unless the scan tables still list this candidate with
    /// the files the write was prepared against; returns its scan generation.
    fn verify(
        &self,
        sql: &SqlContext<'_, '_>,
        content_hash: &str,
        file_revision: u64,
    ) -> Result<u64, DbError> {
        require_current_candidate(
            sql,
            &self.watched_folder_path,
            &self.candidate_path,
            content_hash,
            file_revision,
        )
    }
}

/// Rows a candidate save carries in its transaction beside the candidate's
/// own: the scan-side rows that describe the same file shape the save is
/// checked against, and — for a reset — the lookup choices it clears.
#[derive(Debug, Clone)]
pub(crate) struct CandidateSaveExtras {
    /// The file metadata reading the draft was projected from, stored under the
    /// scan stamp the save was prepared against.
    pub file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
    /// Every folder candidate sharing the hash, with its files settled to
    /// the saved file decisions. Their scan rows are rewritten to this shape
    /// and stamped with the saved file revision.
    pub reshaped_files: Option<Vec<(String, CategorizedFiles)>>,
    pub pane: CandidatePaneWrite,
    /// The save is a person's decision about what the candidate is, which
    /// ends its identification in the same write: no read sees the decision
    /// beside the identification it answers.
    pub ends_identification: bool,
}

/// What this save does to where the candidate's pane stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidatePaneWrite {
    /// The pane stays where the person left it.
    Keep,
    /// The pane opens on the draft identification just applied its pick to,
    /// unless the picked release failed a check against the folder: that is a
    /// question, and the pane stays where the person left it.
    OpenOnDraftUnlessCheckFailed,
}

impl Default for CandidateSaveExtras {
    fn default() -> Self {
        Self {
            file_tag_snapshot: None,
            reshaped_files: None,
            pane: CandidatePaneWrite::Keep,
            ends_identification: false,
        }
    }
}

/// What a run's verdict save did.
#[derive(Debug)]
pub(crate) enum RunVerdictSaved {
    /// The verdict landed, and this is what it did to the run's
    /// identification of the candidate.
    Landed(crate::import::candidate_runtime::IdentificationMove),
    /// The stored revisions had moved past the expectation; nothing was
    /// written.
    Superseded,
}

/// What a save did.
#[derive(Debug)]
pub(crate) enum CandidateSaved {
    /// Every row landed. Carries the scanned candidates a reshape rewrote.
    Landed(Vec<FolderCandidate>),
    /// The stored revisions had moved past the expectation between the load
    /// and this write; nothing was written.
    Superseded,
}

pub(super) fn load_preparation_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Option<CandidatePreparation>, DbError> {
    let Some(state) = load_states_on(sql, CandidateRows::One(content_hash))?.remove(content_hash)
    else {
        return Ok(None);
    };
    let rows = load_pane_rows_on(sql, content_hash)?;
    let assets = prepared_asset_rows::load_asset_rows_on(sql, content_hash)?;
    Ok(Some(CandidatePreparation {
        content_hash: state.content_hash,
        folder_path: state.folder_path,
        file_edits: state.file_edits,
        metadata_revision: state.metadata_revision,
        author: state.metadata_author,
        metadata: crate::import::CandidateMetadataDraft {
            draft: rows.draft,
            provenance: state.metadata_provenance,
            cover: rows.cover,
            cover_options: rows.cover_options,
            assets,
        },
        link: state.release_link,
        identification: state.identify,
        signals: state.signals,
    }))
}

/// Write `prep` whole, provided the stored revisions still match `expected`.
pub(super) fn save_preparation_on(
    sql: &SqlContext<'_, '_>,
    prep: &CandidatePreparation,
    expected: &CandidateSaveExpectation,
    extras: &CandidateSaveExtras,
) -> Result<CandidateSaved, DbError> {
    prep.validate().map_err(DbError::Message)?;
    let content_hash = prep.content_hash.as_str();
    let expected_edit = to_i64(expected.edit_revision, "candidate edit revision")?;
    let expected_metadata = to_i64(expected.metadata_revision, "candidate metadata revision")?;
    let next_edit = to_i64(prep.file_edits.revision, "candidate edit revision")?;
    let next_metadata = to_i64(prep.metadata_revision, "candidate metadata revision")?;

    let changed = sql.execute(
        "UPDATE import_candidate_state SET \
             folder_path = :folder_path, \
             edit_revision = :next_edit, metadata_revision = :next_metadata \
         WHERE content_hash = :content_hash \
           AND edit_revision = :expected_edit AND metadata_revision = :expected_metadata",
        named_params! {
            ":folder_path": prep.folder_path,
            ":next_edit": next_edit,
            ":next_metadata": next_metadata,
            ":content_hash": content_hash,
            ":expected_edit": expected_edit,
            ":expected_metadata": expected_metadata,
        },
    )?;
    if changed != 1 {
        let exists = sql
            .query_row(
                "SELECT 1 FROM import_candidate_state WHERE content_hash = ?",
                [content_hash],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(DbError::Message(format!(
                "candidate {content_hash} has no state row to save into"
            )));
        }
        return Ok(CandidateSaved::Superseded);
    }

    let scan_generation = expected
        .scanned
        .as_ref()
        .map(|scanned| scanned.verify(sql, content_hash, expected.edit_revision))
        .transpose()?;

    let previous_link = super::rows::load_release_link_on(sql, content_hash)?;
    let previous_provenance = super::rows::load_provenance_on(sql, content_hash)?;
    if previous_link != prep.link
        || previous_provenance != prep.metadata.provenance
        || prep.metadata.draft.release_edit().is_blank()
    {
        // An answer belongs to these source credits. Changing their source
        // requires the person to decide any new conflict.
        sql.execute(
            "DELETE FROM import_candidate_artist_identity_choice WHERE content_hash = ?",
            [content_hash],
        )?;
        sql.execute("DELETE FROM import_candidate_failure WHERE content_hash = ? AND content_hash IN (SELECT content_hash FROM import_candidate_artist_identity_conflict)", [content_hash])?;
    }

    // The matches hang off the verdict row, so this clears them too.
    delete_verdict(sql, content_hash)?;
    if let Some(identification) = &prep.identification {
        insert_verdict(sql, content_hash, identification)?;
    }
    delete_signals(sql, content_hash)?;
    if let Some(signals) = &prep.signals {
        insert_signals(sql, content_hash, signals)?;
    }
    super::rows::replace_release_link(sql, content_hash, prep.link.as_ref())?;
    // Replacing the draft row cascades the provenance away;
    // `validate` has already refused an author the provenance cannot have.
    pane_rows::replace_draft(sql, content_hash, &prep.metadata.draft, prep.author)?;
    if let Some(provenance) = prep.metadata.provenance.as_ref() {
        insert_provenance(sql, content_hash, provenance)?;
    }
    pane_rows::delete_cover(sql, content_hash)?;
    if let Some(cover) = &prep.metadata.cover {
        super::candidate_state_rows::save_cover(sql, content_hash, cover)?;
    }
    pane_rows::replace_cover_options(sql, content_hash, &prep.metadata.cover_options)?;
    prepared_asset_rows::replace_asset_rows(sql, content_hash, &prep.metadata.assets)?;
    delete_file_edits(sql, content_hash)?;
    insert_file_edits(sql, content_hash, &prep.file_edits)?;
    match extras.pane {
        CandidatePaneWrite::Keep => {}
        CandidatePaneWrite::OpenOnDraftUnlessCheckFailed => {
            let identification = prep.identification.as_ref().ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {content_hash} opens on a result it stores none of"
                ))
            })?;
            if crate::identify::VerdictSummary::of(
                &identification.verdict,
                identification.kept_own_draft,
            )
            .folder_check()
            .is_none()
            {
                super::session_rows::move_pane_on(
                    sql,
                    content_hash,
                    crate::import::PaneMove::SettledOnPick,
                )?;
            }
        }
    }

    let reshaped = match &extras.reshaped_files {
        None => Vec::new(),
        Some(settled_candidates) => {
            let settled_by_key: HashMap<_, _> = settled_candidates.iter().cloned().collect();
            if settled_by_key.len() != settled_candidates.len() {
                return Err(DbError::Message(
                    "candidate file decision received duplicate scan entry keys".to_string(),
                ));
            }
            settle_scanned_candidates(sql, content_hash, expected_edit, next_edit, &settled_by_key)?
        }
    };
    // The draft holds one track per audio unit, in the folder's order, so a
    // reshape lands only with a draft redrawn over the files it produced.
    for candidate in &reshaped {
        let units = crate::import::audio_layout::audio_units(&candidate.files);
        let drawn = prep
            .metadata
            .draft
            .tracks
            .iter()
            .map(|track| &track.edit.file);
        if !drawn.eq(units.iter()) {
            return Err(DbError::Message(format!(
                "the draft stored for candidate {} is not drawn over its audio",
                candidate.key(),
            )));
        }
    }
    if let Some(snapshot) = &extras.file_tag_snapshot {
        let scanned = expected.scanned.as_ref().ok_or_else(|| {
            DbError::Message("a file-tag snapshot is stored under a scanned candidate key".into())
        })?;
        if snapshot.file_edit_revision != prep.file_edits.revision
            || Some(snapshot.scan_generation) != scan_generation
            || !super::folder_scans::replace_candidate_file_tag_snapshot_on(
                sql,
                &scanned.watched_folder_path,
                &scanned.candidate_path,
                snapshot,
            )?
        {
            return Err(DbError::Message(format!(
                "candidate {} changed before its file tags were stored",
                scanned.candidate_path,
            )));
        }
    }
    if extras.ends_identification {
        super::change_identification_on(
            sql,
            &prep.folder_path,
            &crate::import::candidate_runtime::IdentificationChange::Ended,
        )?;
    }
    Ok(CandidateSaved::Landed(reshaped))
}

impl Database {
    /// One candidate's whole stored state, or `None` for a hash no scan has
    /// stored a candidate under.
    pub(crate) async fn load_candidate_preparation(
        &self,
        content_hash: &str,
    ) -> Result<Option<CandidatePreparation>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| load_preparation_on(&sql, &content_hash))
            .await
    }

    /// Write one candidate's whole state, in one transaction, if its stored
    /// revisions still match what the caller loaded.
    pub(crate) async fn save_candidate_preparation(
        &self,
        prep: CandidatePreparation,
        expected: CandidateSaveExpectation,
        extras: CandidateSaveExtras,
    ) -> Result<CandidateSaved, DbError> {
        self.call(move |sql| save_preparation_on(sql, &prep, &expected, &extras))
            .await
    }

    /// [`Self::save_candidate_preparation`] for the verdict `run` reached:
    /// a save that lands ends that run's identification of the candidate at
    /// `candidate_key` in the same transaction, so no read sees the verdict
    /// beside the run still going, or the run ended without its verdict.
    pub(crate) async fn save_run_verdict(
        &self,
        prep: CandidatePreparation,
        expected: CandidateSaveExpectation,
        extras: CandidateSaveExtras,
        candidate_key: String,
        run: crate::identify::IdentifyRunId,
    ) -> Result<RunVerdictSaved, DbError> {
        self.call(
            move |sql| match save_preparation_on(sql, &prep, &expected, &extras)? {
                CandidateSaved::Landed(_) => {
                    Ok(RunVerdictSaved::Landed(super::change_identification_on(
                        sql,
                        &candidate_key,
                        &crate::import::candidate_runtime::IdentificationChange::AnswerEnded {
                            run,
                        },
                    )?))
                }
                CandidateSaved::Superseded => Ok(RunVerdictSaved::Superseded),
            },
        )
        .await
    }

    /// The instant a write stamps on what it stores.
    pub(crate) fn now(&self) -> DateTime<Utc> {
        chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now())
    }
}
