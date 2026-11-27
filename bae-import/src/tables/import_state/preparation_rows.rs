//! One candidate's whole stored state, loaded and saved as a unit.
//!
//! Every row group under a content hash — the candidate row, its verdict with
//! the matches beneath it, its signals, its draft with its provenance and its
//! cover with the bytes prepared for it, its file decisions — is read into one
//! [`CandidatePreparation`] and written back from one, in one transaction
//! guarded by the revisions the caller loaded. The rules about what a
//! candidate may hold live on the value, not here.

use super::rows::full_state_of;
use super::*;
use crate::folder_scanner::CategorizedFiles;
use crate::folder_scanner::FolderCandidate;
use crate::preparations::{CandidatePreparation, StoredPreparation};
use crate::tables::release_groupings::{load_candidate_entry_on, require_editable_files_on};
use bae_store::statement_optional;

/// The revisions a save was prepared against, and — for a person's write —
/// the candidate it edits.
#[derive(Debug, Clone)]
pub(crate) struct CandidateSaveExpectation {
    pub edit_revision: u64,
    pub metadata_revision: u64,
    /// The key of the candidate the save edits. The save reads its entry and
    /// standing inside its transaction, and is refused unless the scan lists
    /// it with the files the save was prepared against, and it may still be
    /// edited. `None` for a save that edits no listed candidate.
    pub editing: Option<crate::CandidateKey>,
}

/// Rows a candidate save carries in its transaction beside the candidate's
/// own: the scan-side rows that describe the same file shape the save is
/// checked against, and — for a reset — the lookup choices it clears.
#[derive(Debug, Clone, Default)]
pub(crate) struct CandidateSaveExtras {
    /// The file metadata reading the draft was projected from, stored under the
    /// scan stamp the save was prepared against.
    pub file_tag_snapshot: Option<crate::file_tag_snapshot::FileTagSnapshot>,
    /// Every folder candidate sharing the hash, with its files settled to
    /// the saved file decisions. Their scan rows are rewritten to this shape
    /// and stamped with the saved file revision.
    pub reshaped_files: Option<Vec<(crate::CandidateKey, CategorizedFiles)>>,
    /// The save is a person's decision about what the candidate is, which
    /// ends its identification in the same write: no read sees the decision
    /// beside the identification it answers.
    pub ends_identification: bool,
    /// The catalog releases the save's rows may name, as they were read:
    /// each one a row names once the save's rows are written is stored in
    /// the same write, and the rest are not (a release is stored only while
    /// something names it).
    pub releases: Vec<bae_catalogs::source_release::SourceRelease>,
}

/// What a run's verdict save did.
#[derive(Debug)]
pub(crate) enum RunVerdictSaved {
    /// The verdict landed, and this is what it did to the run's
    /// identification of the candidate.
    Landed(crate::candidate_runtime::IdentificationMove),
    /// The stored revisions had moved past the expectation, or the candidate
    /// is no longer one identification may answer with these files; nothing
    /// was written.
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
    /// The cover names a file or tag artwork the candidate's scan did not
    /// store; nothing was written.
    CoverRefused(crate::CoverRefusal),
}

/// What one read of a candidate's whole stored state found: the state, or
/// the release this device already imported from its files, once the
/// candidate holds no draft.
#[derive(Debug)]
pub(crate) enum CandidatePreparationRead {
    Stored(Box<StoredPreparation>),
    Imported { release_id: String },
}

#[cfg(test)]
impl CandidatePreparationRead {
    /// The stored state; `None` for an imported candidate.
    pub(crate) fn stored(self) -> Option<CandidatePreparation> {
        match self {
            Self::Stored(stored) => Some(stored.preparation),
            Self::Imported { .. } => None,
        }
    }
}

// Everything stored under `content_hash` when a candidate holds a state row
// there: the state with its verdict, signals, file decisions, link and lookup
// choices, the release this device imported from its files, its draft, its
// cover with the bytes prepared for it, and the album picked for it.
sqlive_compose::fragment!(
    Preparation(content_hash: String) {},
    r#"
    SELECT state.*, lines.*, origin.*, draft.*, cover.*, pick.*
    FROM @crate::tables::import_state::state_fragments::CurrentState(owner: (
        SELECT stored.content_hash AS content_hash, NULL AS identify_run
        FROM import_candidate_state AS stored
        WHERE stored.content_hash = :content_hash)) AS state
    LEFT JOIN @crate::tables::import_state::state_fragments::SignalTextPool(owner: (
        SELECT stored.content_hash AS content_hash, NULL AS identify_run
        FROM import_candidate_state AS stored
        WHERE stored.content_hash = :content_hash)) AS lines
        ON lines.owner_row = state.owner_row
    LEFT JOIN @crate::tables::import_origin::fragments::ReleaseOrigin AS origin
        ON origin.origin_hash = :content_hash
    LEFT JOIN @crate::tables::import_state::fragments::DraftRows(
        candidate: (SELECT :content_hash AS content_hash)) AS draft ON 1
    LEFT JOIN @crate::tables::import_state::cover_rows::ChosenCover AS cover
        ON cover.prepared_hash = :content_hash
    LEFT JOIN @crate::tables::import_state::album_pick_rows::AlbumPickRow AS pick
        ON pick.pick_hash = :content_hash
"#
);

sqlive_compose::read!(
    PreparationOf(content_hash: String),
    r#"
    SELECT preparation.*
    FROM @crate::tables::import_state::preparation_rows::Preparation(
        content_hash: :content_hash) AS preparation
"#
);

sqlive_compose::read!(
    PreparationWithCardsOf(content_hash: String),
    r#"
    SELECT preparation.*, cards.*
    FROM @crate::tables::import_state::preparation_rows::Preparation(
        content_hash: :content_hash) AS preparation
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictCards(owner: (
        SELECT :content_hash AS content_hash, NULL AS identify_run)) AS cards ON 1
"#
);

/// [`PreparationOf`] on the connection the caller holds, a read's or a
/// write's; `None` for a hash no candidate holds state under.
pub(super) fn load_preparation_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<Option<CandidatePreparationRead>, DbError> {
    let request = PreparationOfParams { content_hash: content_hash.to_string() };
    statement_optional(sql, &request)?.map(|row| preparation_of(row.preparation)).transpose()
}

/// What the row of a read of [`Preparation`] holds. Every state of a
/// candidate this device did not import holds a draft row (the scan seeds it,
/// and deleting the release it imported seeds it again), so a state with none
/// is a damaged one and the read fails.
fn preparation_of(row: Preparation) -> Result<CandidatePreparationRead, DbError> {
    let Preparation { state, lines, origin, draft, cover, pick } = row;
    let state = full_state_of(state, lines)?;
    if let Some(origin) = origin {
        return Ok(CandidatePreparationRead::Imported { release_id: origin.origin_release_id });
    }
    let draft = draft.ok_or_else(|| {
        DbError::unreadable(
            "import_candidate_edit of a candidate not imported here",
            &state.content_hash,
        )
    })?;
    Ok(CandidatePreparationRead::Stored(Box::new(StoredPreparation {
        preparation: CandidatePreparation {
            content_hash: state.content_hash,
            candidate_key: state.candidate_key,
            file_edits: state.file_edits,
            metadata_revision: state.metadata_revision,
            author: state.metadata_author,
            metadata: crate::CandidateMetadataDraft {
                draft: super::pane_rows::draft_of(&draft)?,
                provenance: state.metadata_provenance,
                cover: cover.map(super::cover_rows::ChosenCover::prepared).transpose()?,
            },
            link: state.release_link,
            identification: state.identify,
            signals: state.signals,
        },
        lookup_choices: state.lookup_choices,
        album_pick: super::album_pick_rows::album_pick_of(pick),
    })))
}

/// Why a save of a candidate's whole state was refused. Nothing of it was
/// written.
#[derive(Debug, thiserror::Error)]
pub enum PreparationRefusal {
    /// The candidate holds no state to save over: it left the scan.
    #[error("candidate {0} has no state row to save into")]
    NoState(String),
    /// The candidate's files were imported, and it holds no draft.
    #[error("candidate {0} was imported, and holds no draft to save over")]
    Imported(String),
    /// The settled candidates a file decision hands the save name one scan
    /// entry twice.
    #[error("candidate file decision received duplicate scan entry keys")]
    DuplicateScanKeys,
    /// A reshape's draft is not drawn over the audio the reshape produced.
    #[error("the draft stored for candidate {0} is not drawn over its audio")]
    DraftNotOverAudio(crate::CandidateKey),
    /// A file-tag snapshot came with a save of a candidate no scan keys.
    #[error("a file-tag snapshot is stored under a scanned candidate key")]
    SnapshotOfUnscanned,
    /// The candidate changed after its file tags were read.
    #[error("candidate {0} changed before its file tags were stored")]
    TagsChanged(crate::CandidateKey),
    /// A release the scan holds reading the candidate's files changed, left
    /// or came, after the save was prepared against them.
    #[error("candidate {0} changed before its save was stored")]
    Changed(crate::CandidateKey),
}

/// Whether identification may still answer the candidate at `key` with the
/// verdict `prep` holds: a release that may be worked on, not set aside,
/// imported or claimed by an import, with the files the verdict answers.
fn answers_on(
    sql: &CovenWrite<'_>,
    key: &crate::CandidateKey,
    prep: &CandidatePreparation,
) -> Result<bool, DbError> {
    Ok(load_candidate_entry_on(sql, key)?.is_some_and(|entry| {
        entry.actionable()
            && entry.standing().answerable()
            && entry.candidate.file_edit_revision == prep.file_edits.revision
            && entry.candidate.files.content_hash() == prep.content_hash
    }))
}

/// Write `prep` over what is stored, provided the stored revisions still
/// match `expected`: every row that differs, and no other.
pub(super) fn save_preparation_on(
    sql: &CovenWrite<'_>,
    prep: &CandidatePreparation,
    expected: &CandidateSaveExpectation,
    extras: &CandidateSaveExtras,
) -> Result<CandidateSaved, crate::ImportError> {
    prep.validate()?;
    let content_hash = prep.content_hash.as_str();
    let edited = expected
        .editing
        .as_ref()
        .map(|key| require_editable_files_on(sql, key, content_hash, expected.edit_revision))
        .transpose()?;
    // Before any write, so a refused cover leaves everything as it was.
    if let Some(cover) = &prep.metadata.cover {
        if let Some(refusal) = folder_scans::cover_refusal(
            sql,
            content_hash,
            cover,
            extras.file_tag_snapshot.as_ref(),
        )? {
            return Ok(CandidateSaved::CoverRefused(refusal));
        }
    }
    let expected_edit = to_i64(expected.edit_revision, "candidate edit revision")?;
    let expected_metadata = to_i64(expected.metadata_revision, "candidate metadata revision")?;
    let next_edit = to_i64(prep.file_edits.revision, "candidate edit revision")?;
    let next_metadata = to_i64(prep.metadata_revision, "candidate metadata revision")?;

    let changed = sql.execute(
        "UPDATE import_candidate_state SET \
             candidate_key = :candidate_key, \
             edit_revision = :next_edit, metadata_revision = :next_metadata \
         WHERE content_hash = :content_hash \
           AND edit_revision = :expected_edit AND metadata_revision = :expected_metadata",
        named_params! {
            ":candidate_key": prep.candidate_key,
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
            return Err(PreparationRefusal::NoState(content_hash.to_string()).into());
        }
        return Ok(CandidateSaved::Superseded);
    }

    // What is stored now, which each row group is written over: a group the
    // save leaves as it is writes nothing, and the rows the import list's
    // placement reads (the verdict and its matches, the link, the draft row,
    // its album artists, its provenance) are changed in place, so no
    // statement shows the list the candidate as something it is not.
    let stored = match load_preparation_on(sql, content_hash)? {
        Some(CandidatePreparationRead::Stored(stored)) => stored.preparation,
        Some(CandidatePreparationRead::Imported { .. }) => {
            return Err(PreparationRefusal::Imported(content_hash.to_string()).into())
        }
        None => return Err(PreparationRefusal::NoState(content_hash.to_string()).into()),
    };
    // Store the signals first, because the verdict write checks its rows
    // against their text.
    if stored.signals != prep.signals {
        match &prep.signals {
            Some(signals) => {
                store_signals(sql, super::verdict_rows::Owner::Candidate(content_hash), signals)?;
            }
            // The values hang off the header, so this clears them too.
            None => delete_signals(sql, content_hash)?,
        }
    }
    if stored.identification != prep.identification {
        match &prep.identification {
            Some(identification) => store_verdict(sql, content_hash, identification)?,
            // Deleting the verdict row also deletes the matches that
            // reference it.
            None => delete_verdict(sql, content_hash)?,
        }
    }
    if stored.link != prep.link {
        super::release_link_rows::store_release_link(sql, content_hash, prep.link.as_ref())?;
    }
    // `validate` has already refused an author the provenance cannot have.
    super::draft_store::store_draft(
        sql,
        content_hash,
        (&stored.metadata.draft, stored.author),
        (&prep.metadata.draft, prep.author),
    )?;
    if stored.metadata.provenance != prep.metadata.provenance {
        pane_rows::store_provenance(sql, content_hash, prep.metadata.provenance.as_ref())?;
    }
    if stored.metadata.cover != prep.metadata.cover {
        match &prep.metadata.cover {
            Some(cover) => cover_rows::save_cover(sql, content_hash, cover)?,
            None => cover_rows::delete_cover(sql, content_hash)?,
        }
    }
    if (&stored.file_edits.sheet_bindings, &stored.file_edits.sheet_places)
        != (&prep.file_edits.sheet_bindings, &prep.file_edits.sheet_places)
    {
        delete_file_edits(sql, content_hash)?;
        insert_file_edits(sql, content_hash, &prep.file_edits)?;
    }
    let reshaped = match &extras.reshaped_files {
        None => Vec::new(),
        Some(settled_candidates) => {
            let settled_by_key: HashMap<_, _> = settled_candidates.iter().cloned().collect();
            if settled_by_key.len() != settled_candidates.len() {
                return Err(PreparationRefusal::DuplicateScanKeys.into());
            }
            settle_scanned_candidates(sql, content_hash, expected_edit, next_edit, &settled_by_key)?
        }
    };
    // The draft's tracks play the folder's audio units, each once and in the
    // folder's order, so a reshape lands only with a draft redrawn over the
    // files it produced.
    for candidate in &reshaped {
        let units = crate::audio_layout::audio_units(&candidate.files);
        let mut units = units.iter();
        let mut drawn = prep.metadata.draft.tracks.iter().map(|track| &track.edit.file);
        if !drawn.all(|file| units.any(|unit| unit == file)) {
            return Err(PreparationRefusal::DraftNotOverAudio(candidate.key()).into());
        }
    }
    if let Some(snapshot) = &extras.file_tag_snapshot {
        let (key, entry) = expected
            .editing
            .as_ref()
            .zip(edited.as_ref())
            .ok_or(PreparationRefusal::SnapshotOfUnscanned)?;
        if !prep.file_edits.is_at_revision(snapshot.file_edit_revision)
            || snapshot.scan_generation != entry.generation
            || !super::folder_scans::tag_snapshot::replace_candidate_file_tag_snapshot_on(
                sql,
                &entry.candidate.watched_folder_path,
                key,
                snapshot,
            )?
        {
            return Err(PreparationRefusal::TagsChanged(key.clone()).into());
        }
    }
    // The verdict, link and provenance written above name these, or some
    // of them; the rows naming a release that are gone took it with them.
    crate::tables::source_releases::store_named_releases_on(sql, &extras.releases)?;
    // What the link claims comes from the releases it names, which this
    // write stored above.
    if stored.link != prep.link {
        super::claim_rows::store_claims_on(sql, content_hash)?;
    }
    if extras.ends_identification {
        super::change_identification_on(
            sql,
            &prep.candidate_key,
            &crate::candidate_runtime::IdentificationChange::Ended,
        )?;
    }
    Ok(CandidateSaved::Landed(reshaped))
}

impl ImportTables {
    /// One candidate's whole stored state, or the release this device
    /// imported from its files, or `None` for a hash no scan has stored a
    /// candidate under.
    pub(crate) async fn load_candidate_preparation(
        &self,
        content_hash: &str,
    ) -> Result<Option<CandidatePreparationRead>, DbError> {
        let content_hash = content_hash.to_string();
        self.store.read(move |sql| load_preparation_on(&sql, &content_hash)).await
    }

    /// [`Self::load_candidate_preparation`] with the cards of the verdict it
    /// holds, as one statement.
    pub(crate) async fn load_candidate_preparation_with_cards(
        &self,
        content_hash: &str,
    ) -> Result<Option<(CandidatePreparationRead, bae_identify::VerdictCards)>, DbError> {
        let request = PreparationWithCardsOfParams { content_hash: content_hash.to_string() };
        let Some(row) = self.store.read_rows(request).await?.pop() else {
            return Ok(None);
        };
        let cards = super::cards_read::verdict_cards_of_column(
            row.cards.and_then(|cards| cards.verdict_cards).as_deref(),
        )?;
        Ok(Some((preparation_of(row.preparation)?, cards)))
    }

    /// Write one candidate's whole state, in one transaction, if its stored
    /// revisions still match what the caller loaded.
    pub(crate) async fn save_candidate_preparation(
        &self,
        prep: CandidatePreparation,
        expected: CandidateSaveExpectation,
        extras: CandidateSaveExtras,
    ) -> Result<CandidateSaved, crate::ImportError> {
        self.store.write(move |sql| save_preparation_on(&sql, &prep, &expected, &extras)).await
    }

    /// [`Self::save_candidate_preparation`] for the verdict `run` reached:
    /// a save that lands ends that run's identification of the candidate at
    /// `candidate_key` in the same transaction, so no read sees the verdict
    /// beside the run still going, or the run ended without its verdict.
    /// Nothing lands unless that transaction reads the candidate as one
    /// identification may still answer, with the files the verdict answers.
    pub(crate) async fn save_run_verdict(
        &self,
        prep: CandidatePreparation,
        expected: CandidateSaveExpectation,
        extras: CandidateSaveExtras,
        candidate_key: crate::CandidateKey,
        run: crate::IdentifyRunId,
    ) -> Result<RunVerdictSaved, crate::ImportError> {
        self.store
            .write(move |sql| {
                if !answers_on(&sql, &candidate_key, &prep)? {
                    return Ok(RunVerdictSaved::Superseded);
                }
                match save_preparation_on(&sql, &prep, &expected, &extras)? {
                    CandidateSaved::Landed(_) => {
                        Ok(RunVerdictSaved::Landed(super::change_identification_on(
                            &sql,
                            &candidate_key,
                            &crate::candidate_runtime::IdentificationChange::AnswerEnded { run },
                        )?))
                    }
                    CandidateSaved::Superseded => Ok(RunVerdictSaved::Superseded),
                    // A run's verdict brings a catalog's cover or keeps the
                    // one stored, which its own save checked.
                    CandidateSaved::CoverRefused(refusal) => Err(crate::ImportError::from(refusal)),
                }
            })
            .await
    }
}

#[cfg(test)]
#[path = "preparation_rows_tests.rs"]
mod tests;
