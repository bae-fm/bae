//! Reading a candidate back: its row, its verdict with the matches that hang
//! off it, its draft's provenance, and its file decisions.

use super::edit_rows::{apply_file_edit_row, read_file_edit_row};
use super::signal_rows::load_signals_on;
use super::verdict_rows::{
    identification_of, read_match_row, read_verdict_row, unreadable, StoredMatches, VERDICT_COLUMNS,
};
use super::*;
use crate::import::{MetadataAuthor, MetadataProvenance, MetadataRef, MetadataSource};
use std::str::FromStr;

/// The provenance as its columns. Only an external release names a source and
/// a release; `author` is never absent, because a provenance nobody put there
/// has no row at all.
struct ProvenanceColumns<'a> {
    kind: &'static str,
    source: Option<&'static str>,
    release_id: Option<&'a str>,
    author: &'static str,
}

fn provenance_columns<'a>(
    provenance: &'a MetadataProvenance,
    author: &'static str,
) -> ProvenanceColumns<'a> {
    match provenance {
        MetadataProvenance::FileTags => ProvenanceColumns {
            kind: "file_tags",
            source: None,
            release_id: None,
            author,
        },
        MetadataProvenance::ExternalRelease {
            source, release_id, ..
        } => ProvenanceColumns {
            kind: "external_release",
            source: Some(source.as_str()),
            release_id: Some(release_id.as_str()),
            author,
        },
    }
}

/// The column an author is stored as, or `None` for a draft nobody chose —
/// which has no provenance row, so nothing is written for it.
pub(super) fn author_column(author: MetadataAuthor) -> Option<&'static str> {
    match author {
        MetadataAuthor::Nobody => None,
        MetadataAuthor::Identification => Some("identification"),
        MetadataAuthor::User => Some("user"),
    }
}

fn author_of(stored: &str) -> Result<MetadataAuthor, DbError> {
    match stored {
        "identification" => Ok(MetadataAuthor::Identification),
        "user" => Ok(MetadataAuthor::User),
        other => Err(unreadable("provenance author", other)),
    }
}

/// Write the draft's provenance, its author, and the partner releases the same
/// pick claimed.
///
/// Called after the draft row is replaced, which cascades the previous
/// provenance and its partners away, so this only ever inserts.
pub(super) fn insert_provenance(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    provenance: &MetadataProvenance,
    author: &'static str,
) -> Result<(), DbError> {
    let columns = provenance_columns(provenance, author);
    sql.execute(
        "INSERT INTO import_candidate_draft_provenance \
             (content_hash, kind, source, release_id, author) \
         VALUES (?, ?, ?, ?, ?)",
        params![
            content_hash,
            columns.kind,
            columns.source,
            columns.release_id,
            columns.author
        ],
    )?;
    let MetadataProvenance::ExternalRelease { partners, .. } = provenance else {
        return Ok(());
    };
    for partner in partners {
        sql.execute(
            "INSERT INTO import_candidate_provenance_partner (content_hash, source, release_id) \
             VALUES (?, ?, ?)",
            params![content_hash, partner.source.as_str(), partner.id],
        )?;
    }
    Ok(())
}

/// Every candidate's draft provenance with its author, or the one `only`
/// names. The partner rows are read in the same pass, since only the
/// provenance they belong to explains them.
pub(crate) fn load_provenance_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, (MetadataProvenance, MetadataAuthor)>, DbError> {
    let mut partners: HashMap<String, Vec<MetadataRef>> = HashMap::new();
    for (content_hash, source, release_id) in sql.query(
        "SELECT content_hash, source, release_id FROM import_candidate_provenance_partner \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, source",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )? {
        let source = MetadataSource::from_str(&source).map_err(DbError::Message)?;
        partners
            .entry(content_hash)
            .or_default()
            .push(MetadataRef::new(release_id, source));
    }

    let rows = sql.query(
        "SELECT content_hash, kind, source, release_id, author \
         FROM import_candidate_draft_provenance \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?;
    let mut out = HashMap::with_capacity(rows.len());
    for (content_hash, kind, source, release_id, author) in rows {
        let partners = partners.remove(&content_hash).unwrap_or_default();
        let provenance = match kind.as_str() {
            "file_tags" => MetadataProvenance::FileTags,
            "external_release" => {
                let missing = |what: &str| {
                    DbError::Message(format!(
                        "stored external metadata provenance names no {what}"
                    ))
                };
                MetadataProvenance::ExternalRelease {
                    source: MetadataSource::from_str(&source.ok_or_else(|| missing("source"))?)
                        .map_err(DbError::Message)?,
                    release_id: release_id.ok_or_else(|| missing("release"))?,
                    partners,
                }
            }
            other => return Err(unreadable("provenance kind", other)),
        };
        out.insert(content_hash, (provenance, author_of(&author)?));
    }
    Ok(out)
}

/// Every candidate's verdict, or the one `only` names, each rebuilt with the
/// matches that hang off it.
pub(crate) fn load_verdicts_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, DbCandidateIdentifyResult>, DbError> {
    let mut matches = load_matches_on(sql, only)?;
    let rows = sql.query(
        &format!(
            "SELECT {VERDICT_COLUMNS} FROM import_candidate_verdict \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
        |row| Ok(read_verdict_row(row)),
    )?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let row = row?;
        let content_hash = row.content_hash.clone();
        let found = matches.remove(&content_hash).unwrap_or_default();
        out.insert(content_hash, identification_of(row, found)?);
    }
    Ok(out)
}

struct StateRow {
    content_hash: String,
    folder_path: String,
    edit_revision: i64,
    metadata_revision: i64,
}

fn read_state_row(row: &Row<'_>) -> Result<StateRow, DbError> {
    Ok(StateRow {
        content_hash: row.get("content_hash")?,
        folder_path: row.get("folder_path")?,
        edit_revision: row.get("edit_revision")?,
        metadata_revision: row.get("metadata_revision")?,
    })
}

const STATE_COLUMNS: &str = "content_hash, folder_path, edit_revision, metadata_revision";

const MATCH_COLUMNS: &str = "content_hash, source, release_id, title, artist, year, \
     format, label, catalog_number, country, barcode, cover_url, cover_thumbnail_url, \
     cover_label, cover_source, source_group_id, source_tracks_kind, source_tracks_count, \
     source_tracks_total_ms, by_disc_id, by_barcode, by_catalog";

const FILE_EDIT_COLUMNS: &str = "content_hash, relative_path, role_choice, sheet_binding, \
     sheet_binding_file_id, sheet_disc, sheet_disc_number";

/// Every candidate's stored matches, keyed by content hash, or just the one
/// `only` names.
///
/// The whole row per match rather than a count and a lead: how many *pressings*
/// a verdict named is decided by grouping them, which needs the fields each one
/// states. The one reader of these columns, for the pane's whole verdict and
/// for the queue list's summary alike.
pub(crate) fn load_matches_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, StoredMatches>, DbError> {
    let mut matches: HashMap<String, StoredMatches> = HashMap::new();
    for row in sql.query(
        &format!(
            "SELECT {MATCH_COLUMNS} FROM import_candidate_match \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, position"
        ),
        named_params! { ":only": only },
        |row| Ok(read_match_row(row)),
    )? {
        let row = row?;
        matches
            .entry(row.content_hash)
            .or_default()
            .push((row.result, row.provenance));
    }
    Ok(matches)
}

/// Every stored candidate row, or the one `only` names, assembled with the
/// verdict, signals, provenance, and file-decision rows that hang off it.
pub(crate) fn load_states_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    let states = sql.query(
        &format!(
            "SELECT {STATE_COLUMNS} FROM import_candidate_state \
             WHERE :only IS NULL OR content_hash = :only"
        ),
        named_params! { ":only": only },
        |row| Ok(read_state_row(row)),
    )?;
    let mut verdicts = load_verdicts_on(sql, only)?;
    let mut edits = load_edits_on(sql, only)?;
    let mut signals = load_signals_on(sql, only)?;
    let mut provenances = load_provenance_on(sql, only)?;

    let mut out = HashMap::with_capacity(states.len());
    for state in states {
        let state = state?;
        let mut file_edits = edits.remove(&state.content_hash).unwrap_or_default();
        file_edits.revision = u64::try_from(state.edit_revision).map_err(|_| {
            DbError::Message(format!(
                "import candidate {} has a negative edit revision",
                state.content_hash
            ))
        })?;
        let metadata_revision = u64::try_from(state.metadata_revision).map_err(|_| {
            DbError::Message(format!(
                "import candidate {} has a negative metadata revision",
                state.content_hash
            ))
        })?;
        out.insert(
            state.content_hash.clone(),
            DbImportCandidateState {
                signals: signals.remove(&state.content_hash),
                identify: verdicts.remove(&state.content_hash),
                metadata_provenance: provenances
                    .remove(&state.content_hash)
                    .map(|(provenance, _)| provenance),
                content_hash: state.content_hash,
                folder_path: state.folder_path,
                file_edits,
                metadata_revision,
            },
        );
    }
    Ok(out)
}

/// The per-file decisions of every candidate, or of the one `only` names.
/// `CandidateFileEdits::revision` is left at zero — it lives on the state row,
/// which is what fills it in.
fn load_edits_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, CandidateFileEdits>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT {FILE_EDIT_COLUMNS} FROM import_candidate_file_edit \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, relative_path"
        ),
        named_params! { ":only": only },
        |row| Ok(read_file_edit_row(row)),
    )?;
    let mut edits: HashMap<String, CandidateFileEdits> = HashMap::new();
    for row in rows {
        let row = row?;
        let entry = edits.entry(row.content_hash.clone()).or_default();
        apply_file_edit_row(entry, row)?;
    }
    Ok(edits)
}

/// One candidate's file decisions, revision included. Progressive scans call
/// this after they compute a content hash, so each emitted row performs one
/// indexed lookup instead of rereading the whole table.
pub(super) fn load_candidate_file_edits_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<CandidateFileEdits, DbError> {
    let revision: Option<i64> = sql
        .query_row(
            "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| row.get(0),
        )
        .optional()?;
    let Some(revision) = revision else {
        return Ok(CandidateFileEdits::default());
    };
    let mut edits = load_edits_on(sql, Some(content_hash))?
        .remove(content_hash)
        .unwrap_or_default();
    edits.revision = u64::try_from(revision).map_err(|_| {
        DbError::Message(format!(
            "import candidate {content_hash} has a negative edit revision"
        ))
    })?;
    Ok(edits)
}
