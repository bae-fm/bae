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
use crate::import::preparation::CandidatePreparation;
use crate::import::folder_scanner::FolderCandidate;

/// The revisions a save was prepared against, and — for a write that must
/// not land on a folder the scan has since re-read — where the scan lists it.
#[derive(Debug, Clone)]
pub(crate) struct CandidateSaveExpectation {
    pub edit_revision: u64,
    pub metadata_revision: u64,
    pub scanned: Option<CandidateScanExpectation>,
}

/// Where the scan tables list a candidate: the root it is under and its key.
#[derive(Debug, Clone)]
pub(crate) struct ScannedCandidateKey {
    pub watched_folder_path: String,
    pub candidate_path: String,
}

/// Whether a write only pins the current source or also the scan it read.
#[derive(Debug, Clone)]
pub(crate) enum CandidateScanExpectation {
    Current(ScannedCandidateKey),
    AtGeneration {
        key: ScannedCandidateKey,
        generation: u64,
    },
}

impl CandidateScanExpectation {
    fn key(&self) -> &ScannedCandidateKey {
        match self {
            Self::Current(key) | Self::AtGeneration { key, .. } => key,
        }
    }

    fn verify(
        &self,
        sql: &SqlContext<'_, '_>,
        content_hash: &str,
        file_revision: u64,
    ) -> Result<u64, DbError> {
        let key = self.key();
        let current = require_current_candidate(
            sql,
            &key.watched_folder_path,
            &key.candidate_path,
            content_hash,
            file_revision,
        )?;
        if let Self::AtGeneration { generation, .. } = self {
            if current != *generation {
                return Err(DbError::Message(format!(
                    "candidate {} changed scan generation before its setup was stored",
                    key.candidate_path,
                )));
            }
        }
        Ok(current)
    }
}

/// Rows a candidate save carries in its transaction beside the candidate's
/// own: the scan-side rows that describe the same file shape the save is
/// checked against, and the choices a pick in the save confirms.
#[derive(Debug, Clone)]
pub(crate) struct CandidateSaveExtras {
    /// The file metadata reading the draft was projected from, stored under the
    /// scan stamp the save was prepared against.
    pub file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
    /// Every folder candidate sharing the hash, with its files settled to
    /// the saved file decisions. Their scan rows are rewritten to this shape
    /// and stamped with the saved file revision.
    pub reshaped_files: Option<Vec<(String, CategorizedFiles)>>,
    pub lookup_update: CandidateLookupUpdate,
    pub pane: CandidatePaneWrite,
    /// Whether the verdict this save stores owes an import: an automatic run
    /// settled on it while "Import automatically when identified" was on. Owed
    /// only when the verdict needs nothing from anyone — one that asks
    /// something owes nothing to answer — and owed for the draft this save
    /// leaves.
    pub owes_import: bool,
}

/// What this save does to where the candidate's pane stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidatePaneWrite {
    /// The pane stays where the person left it.
    Keep,
    /// The pane opens on the draft when the result this save stores asks
    /// nothing: identification applied its own pick, and the draft and its
    /// Import are all there is left to see.
    OpenOnDraftIfReady,
}

/// How this save affects the candidate's identification choices.
#[derive(Debug, Clone)]
pub(crate) enum CandidateLookupUpdate {
    Keep,
    /// Confirm a catalog number shared by the selected record and source text.
    ConfirmPick,
    /// Return every lookup choice to its initial value with the setup reset.
    Reset {
        expected: crate::import::LookupChoices,
    },
}

impl Default for CandidateSaveExtras {
    fn default() -> Self {
        Self {
            file_tag_snapshot: None,
            reshaped_files: None,
            lookup_update: CandidateLookupUpdate::Keep,
            pane: CandidatePaneWrite::Keep,
            owes_import: false,
        }
    }
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
    let Some(state) = load_states_on(sql, Some(content_hash))?.remove(content_hash) else {
        return Ok(None);
    };
    let rows = load_pane_rows_on(sql, content_hash)?;
    let source_discogs_artist_ids =
        prepared_asset_rows::load_source_artist_ids_on(sql, content_hash)?;
    let (assets, assets_prepared) = prepared_asset_rows::load_asset_rows_on(sql, content_hash)?;
    Ok(Some(CandidatePreparation {
        content_hash: state.content_hash,
        folder_path: state.folder_path,
        file_edits: state.file_edits,
        metadata_revision: state.metadata_revision,
        author: state.metadata_author,
        metadata: crate::import::CandidateMetadataDraft {
            draft: rows.draft,
            source_discogs_artist_ids,
            provenance: state.metadata_provenance,
            cover: rows.cover,
            assets,
        },
        assets_prepared,
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
    observed_at: i64,
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

    // The matches hang off the verdict row, so this clears them too — and
    // the import it owed, which was owed for the verdict and draft this save
    // replaces.
    delete_verdict(sql, content_hash)?;
    if let Some(identification) = &prep.identification {
        insert_verdict(sql, content_hash, identification)?;
    }
    if extras.owes_import {
        let identification = prep.identification.as_ref().ok_or_else(|| {
            DbError::Message(format!(
                "candidate {content_hash} owes an import for a result it stores none of"
            ))
        })?;
        if crate::identify::classify(&identification.verdict)
            == crate::identify::QueueClassification::Ready
        {
            owed_import_rows::owe_import_on(sql, content_hash, next_metadata)?;
        }
    }
    delete_signals(sql, content_hash)?;
    if let Some(signals) = &prep.signals {
        insert_signals(sql, content_hash, signals)?;
    }
    // Replacing the draft row cascades the provenance and its partners away;
    // `validate` has already refused an author the provenance cannot have.
    pane_rows::replace_draft(sql, content_hash, &prep.metadata.draft, prep.author)?;
    if let Some(provenance) = prep.metadata.provenance.as_ref() {
        insert_provenance(sql, content_hash, provenance)?;
    }
    pane_rows::delete_cover(sql, content_hash)?;
    if let Some(cover) = &prep.metadata.cover {
        super::candidate_state_rows::save_cover(sql, content_hash, cover)?;
    }
    prepared_asset_rows::replace_asset_rows(
        sql,
        content_hash,
        &prep.metadata.source_discogs_artist_ids,
        &prep.metadata.assets,
        prep.assets_prepared,
    )?;
    delete_file_edits(sql, content_hash)?;
    insert_file_edits(sql, content_hash, &prep.file_edits)?;
    match &extras.lookup_update {
        CandidateLookupUpdate::Keep => {}
        CandidateLookupUpdate::ConfirmPick => {
            let current = lookup_choice_rows::load_lookup_choice_rows_on(sql, Some(content_hash))?
                .assemble()
                .remove(content_hash)
                .unwrap_or_default();
            if let Some(confirmed) = prep.choices_confirming_pick(&current) {
                lookup_choice_rows::replace_lookup_choices_on(sql, content_hash, &confirmed)?;
            }
        }
        CandidateLookupUpdate::Reset { expected } => {
            let current = lookup_choice_rows::load_lookup_choice_rows_on(sql, Some(content_hash))?
                .assemble()
                .remove(content_hash)
                .unwrap_or_default();
            if &current != expected {
                return Err(DbError::Message(
                    "candidate lookup choices changed before its setup was reset".into(),
                ));
            }
            lookup_choice_rows::replace_lookup_choices_on(
                sql,
                content_hash,
                &crate::import::LookupChoices::default(),
            )?;
        }
    }

    match extras.pane {
        CandidatePaneWrite::Keep => {}
        CandidatePaneWrite::OpenOnDraftIfReady => {
            let identification = prep.identification.as_ref().ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {content_hash} opens on a result it stores none of"
                ))
            })?;
            if crate::identify::classify(&identification.verdict)
                == crate::identify::QueueClassification::Ready
            {
                super::session_rows::present_on(
                    sql,
                    content_hash,
                    crate::import::MetadataPresentation::Draft,
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
            settle_scanned_candidates(
                sql,
                content_hash,
                expected_edit,
                next_edit,
                &settled_by_key,
                observed_at,
            )?
        }
    };
    for candidate in &reshaped {
        let available = crate::import::track_slots::audio_units(&candidate.files);
        if let Some(track) = prep
            .metadata
            .draft
            .tracks
            .iter()
            .find(|track| !available.contains(&track.edit.file))
        {
            return Err(DbError::Message(format!(
                "candidate {} does not expose the prepared audio {:?}",
                candidate.key(),
                track.edit.file,
            )));
        }
    }
    if let Some(snapshot) = &extras.file_tag_snapshot {
        let scanned = expected
            .scanned
            .as_ref()
            .ok_or_else(|| {
                DbError::Message(
                    "a file-tag snapshot is stored under a scanned candidate key".into(),
                )
            })?
            .key();
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
    Ok(CandidateSaved::Landed(reshaped))
}

fn to_i64(value: u64, what: &str) -> Result<i64, DbError> {
    i64::try_from(value)
        .map_err(|_| DbError::Message(format!("{what} {value} exceeds SQLite's integer range")))
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
        let observed_at = self.inner.clock.now().timestamp_millis();
        self.call(move |sql| save_preparation_on(sql, &prep, &expected, &extras, observed_at))
            .await
    }

    /// The instant a write stamps on what it stores.
    pub(crate) fn now(&self) -> DateTime<Utc> {
        self.inner.clock.now()
    }
}
