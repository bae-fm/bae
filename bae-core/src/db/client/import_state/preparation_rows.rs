//! One candidate's whole stored state, loaded and saved as a unit.
//!
//! Every row group under a content hash — the state row, its matches and
//! failure, its signals, its draft and cover and prepared assets, its file
//! decisions — is read into one [`CandidatePreparation`] and written back
//! from one, in one transaction guarded by the revisions the caller loaded.
//! The rules about what a candidate may hold live on the value, not here.

use super::*;
use crate::import::folder_scanner::{CategorizedFiles, FolderCandidate};
use crate::import::preparation::{CandidatePreparation, MetadataAuthor};

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

/// Scan-side rows a candidate save carries in its transaction, because they
/// describe the same file shape the save is checked against.
#[derive(Debug, Clone, Default)]
pub(crate) struct CandidateSaveExtras {
    /// The File Tags reading the draft was projected from, stored under the
    /// scan stamp the save was prepared against.
    pub file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
    /// Every scanned candidate sharing the hash, with its files settled to
    /// the saved file decisions. Their scan rows are rewritten to this shape
    /// and stamped with the saved file revision.
    pub reshaped_files: Option<Vec<(String, CategorizedFiles)>>,
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

fn author_column(author: MetadataAuthor) -> Option<&'static str> {
    match author {
        MetadataAuthor::Nobody => None,
        MetadataAuthor::Identification => Some(MetadataProvenanceAuthor::Identification.as_str()),
        MetadataAuthor::User => Some(MetadataProvenanceAuthor::User.as_str()),
    }
}

fn author_of(stored: Option<String>) -> Result<MetadataAuthor, DbError> {
    Ok(match stored.as_deref() {
        None => MetadataAuthor::Nobody,
        Some("identification") => MetadataAuthor::Identification,
        Some("user") => MetadataAuthor::User,
        Some(other) => return Err(verdict_rows::unreadable("provenance_author", other)),
    })
}

pub(super) fn load_preparation_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Option<CandidatePreparation>, DbError> {
    let Some(state) = load_states_on(sql, Some(content_hash))?.remove(content_hash) else {
        return Ok(None);
    };
    let author = author_of(sql.query_row(
        "SELECT provenance_author FROM import_candidate_state WHERE content_hash = ?",
        [content_hash],
        |row| row.get::<_, Option<String>>(0),
    )?)?;
    let rows = load_pane_rows_on(sql, content_hash)?;
    let source_discogs_artist_ids =
        prepared_asset_rows::load_source_artist_ids_on(sql, content_hash)?;
    let (assets, assets_prepared) = prepared_asset_rows::load_asset_rows_on(sql, content_hash)?;
    Ok(Some(CandidatePreparation {
        content_hash: state.content_hash,
        folder_path: state.folder_path,
        file_edits: state.file_edits,
        metadata_revision: state.metadata_revision,
        author,
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
) -> Result<CandidateSaved, DbError> {
    prep.validate().map_err(DbError::Message)?;
    let content_hash = prep.content_hash.as_str();
    let expected_edit = to_i64(expected.edit_revision, "candidate edit revision")?;
    let expected_metadata = to_i64(expected.metadata_revision, "candidate metadata revision")?;
    let next_edit = to_i64(prep.file_edits.revision, "candidate edit revision")?;
    let next_metadata = to_i64(prep.metadata_revision, "candidate metadata revision")?;

    if let Some(scanned) = &expected.scanned {
        let generation = require_current_candidate(
            sql,
            &scanned.watched_folder_path,
            &scanned.candidate_path,
            content_hash,
            expected.edit_revision,
        )?;
        if let Some(snapshot) = &extras.file_tag_snapshot {
            if snapshot.file_edit_revision != expected.edit_revision
                || snapshot.scan_generation != generation
            {
                return Err(DbError::Message(format!(
                    "candidate {} changed before its file tags were stored",
                    scanned.candidate_path
                )));
            }
            super::folder_scans::write::replace_candidate_file_tag_snapshot(
                sql,
                &scanned.watched_folder_path,
                &scanned.candidate_path,
                snapshot,
            )?;
        }
    } else if extras.file_tag_snapshot.is_some() {
        return Err(DbError::Message(
            "a file-tag snapshot is stored under a scanned candidate key".into(),
        ));
    }

    let columns = prep
        .identification
        .as_ref()
        .map(|identification| verdict_columns(&identification.verdict));
    let stored_kind = columns.as_ref().and_then(|columns| columns.kind);
    let probed = prep
        .identification
        .as_ref()
        .map(|identification| {
            to_i64(
                identification.probed_total_duration_ms,
                "candidate probed total",
            )
        })
        .transpose()?;
    let identified_at = prep
        .identification
        .as_ref()
        .map(|identification| identification.identified_at.to_rfc3339());
    let seed = prep.metadata.provenance.as_ref().map(seed_columns);
    let changed = sql.execute(
        "UPDATE import_candidate_state SET \
             folder_path = :folder_path, \
             verdict_kind = :kind, verdict_track_count = :track_count, \
             verdict_matched_barcode = :matched_barcode, \
             probed_total_duration_ms = :probed, identified_at = :identified_at, \
             provenance_kind = :provenance_kind, provenance_source = :provenance_source, \
             provenance_release_id = :provenance_release_id, \
             provenance_author = :provenance_author, \
             edit_revision = :next_edit, metadata_revision = :next_metadata \
         WHERE content_hash = :content_hash \
           AND edit_revision = :expected_edit AND metadata_revision = :expected_metadata",
        named_params! {
            ":folder_path": prep.folder_path,
            ":kind": stored_kind,
            ":track_count": columns.as_ref().and_then(|columns| columns.track_count),
            ":matched_barcode": columns.as_ref().and_then(|columns| columns.matched_barcode),
            ":probed": stored_kind.and(probed),
            ":identified_at": stored_kind.and(identified_at.as_deref()),
            ":provenance_kind": seed.as_ref().map(|seed| seed.kind),
            ":provenance_source": seed.as_ref().and_then(|seed| seed.source),
            ":provenance_release_id": seed.as_ref().and_then(|seed| seed.release_id),
            ":provenance_author": author_column(prep.author),
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

    delete_matches(sql, content_hash)?;
    delete_identify_failure(sql, content_hash)?;
    if let Some(identification) = &prep.identification {
        insert_matches(sql, content_hash, &identification.verdict)?;
        insert_identify_failure(
            sql,
            content_hash,
            &identification.verdict,
            probed.expect("a stored verdict carries its probed total"),
            identified_at
                .as_deref()
                .expect("a stored verdict carries its timestamp"),
        )?;
    }
    delete_signals(sql, content_hash)?;
    if let Some(signals) = &prep.signals {
        insert_signals(sql, content_hash, signals)?;
    }
    replace_provenance_partners(sql, content_hash, prep.metadata.provenance.as_ref())?;
    pane_rows::replace_draft(sql, content_hash, &prep.metadata.draft)?;
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
        self.call(move |sql| save_preparation_on(sql, &prep, &expected, &extras))
            .await
    }

    /// The instant a write stamps on what it stores.
    pub(super) fn now(&self) -> DateTime<Utc> {
        self.inner.clock.now()
    }
}
