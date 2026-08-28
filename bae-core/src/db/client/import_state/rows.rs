//! Reading `import_candidate_state` back: the identify columns with their
//! match rows, the metadata draft and provenance, and the per-file decisions.

use super::duration_rows::load_durations_on;
use super::edit_rows::{apply_file_edit_row, read_file_edit_row};
use super::signal_rows::load_signals_on;
use super::verdict_rows::{read_match_row, unreadable, verdict_of, MatchLists};
use super::*;
use crate::import::{MetadataProvenance, MetadataSource};
use std::str::FromStr;

/// Who decided a candidate's stored metadata provenance. Identification's
/// choice goes with its verdict; an explicitly applied source outlives every
/// verdict, so the row records which one it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MetadataProvenanceAuthor {
    User,
    Identification,
}

impl MetadataProvenanceAuthor {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Identification => "identification",
        }
    }
}

/// The metadata provenance as its three columns. Only an external release carries a
/// source and release id.
pub(super) struct SeedColumns<'a> {
    pub(super) kind: &'static str,
    pub(super) source: Option<&'static str>,
    pub(super) release_id: Option<&'a str>,
}

pub(super) fn seed_columns(seed: &MetadataProvenance) -> SeedColumns<'_> {
    match seed {
        MetadataProvenance::FileTags => SeedColumns {
            kind: "file_tags",
            source: None,
            release_id: None,
        },
        MetadataProvenance::ExternalRelease { source, release_id } => SeedColumns {
            kind: "external_release",
            source: Some(source.as_str()),
            release_id: Some(release_id.as_str()),
        },
    }
}

pub(crate) fn metadata_provenance_of(
    kind: Option<String>,
    source: Option<String>,
    release_id: Option<String>,
) -> Result<Option<MetadataProvenance>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    match kind.as_str() {
        "file_tags" => Ok(Some(MetadataProvenance::FileTags)),
        "external_release" => {
            let missing = |what: &str| {
                DbError::Message(format!(
                    "stored external metadata provenance names no {what}"
                ))
            };
            Ok(Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::from_str(&source.ok_or_else(|| missing("source"))?)
                    .map_err(DbError::Message)?,
                release_id: release_id.ok_or_else(|| missing("release"))?,
            }))
        }
        other => Err(unreadable("provenance_kind", other)),
    }
}

struct StateRow {
    content_hash: String,
    folder_path: String,
    verdict_kind: Option<String>,
    verdict_track_count: Option<i64>,
    verdict_matched_barcode: Option<String>,
    probed_total_duration_ms: Option<i64>,
    identified_at: Option<DateTime<Utc>>,
    metadata_provenance: Option<MetadataProvenance>,
    edit_revision: i64,
}

fn read_state_row(row: &Row<'_>) -> Result<StateRow, DbError> {
    let identified_at: Option<String> = row.get("identified_at")?;
    Ok(StateRow {
        content_hash: row.get("content_hash")?,
        folder_path: row.get("folder_path")?,
        verdict_kind: row.get("verdict_kind")?,
        verdict_track_count: row.get("verdict_track_count")?,
        verdict_matched_barcode: row.get("verdict_matched_barcode")?,
        probed_total_duration_ms: row.get("probed_total_duration_ms")?,
        identified_at: identified_at
            .map(|_| rfc3339_column(row, "identified_at"))
            .transpose()?,
        metadata_provenance: metadata_provenance_of(
            row.get("provenance_kind")?,
            row.get("provenance_source")?,
            row.get("provenance_release_id")?,
        )?,
        edit_revision: row.get("edit_revision")?,
    })
}

const STATE_COLUMNS: &str = "content_hash, folder_path, verdict_kind, verdict_track_count, \
     verdict_matched_barcode, probed_total_duration_ms, identified_at, provenance_kind, \
     provenance_source, provenance_release_id, edit_revision";

const MATCH_COLUMNS: &str = "content_hash, source, release_id, title, artist, year, \
     format, label, catalog_number, country, cover_url, cover_thumbnail_url, cover_label, \
     cover_source, source_group_id, source_tracks_kind, source_tracks_count, \
     source_tracks_total_ms, by_disc_id, by_barcode, by_catalog";

const FILE_EDIT_COLUMNS: &str = "content_hash, relative_path, role_choice, sheet_binding, \
     sheet_binding_file_id, sheet_disc, sheet_disc_number";

/// Every stored candidate row, or the one `only` names, assembled with the
/// match and file-decision rows that hang off it.
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
    let matches = sql.query(
        &format!(
            "SELECT {MATCH_COLUMNS} FROM import_candidate_match \
             WHERE :only IS NULL OR content_hash = :only \
             ORDER BY content_hash, position"
        ),
        named_params! { ":only": only },
        |row| Ok(read_match_row(row)),
    )?;
    let mut lists: HashMap<String, MatchLists> = HashMap::new();
    for row in matches {
        let row = row?;
        lists.entry(row.content_hash.clone()).or_default().push(row);
    }
    let mut edits = load_edits_on(sql, only)?;
    let mut durations = load_durations_on(sql, only)?;
    let mut signals = load_signals_on(sql, only, &durations)?;

    let mut out = HashMap::with_capacity(states.len());
    for state in states {
        let state = state?;
        let identify = match (
            state.verdict_kind,
            state.probed_total_duration_ms,
            state.identified_at,
        ) {
            (Some(kind), Some(probed), Some(identified_at)) => Some(DbCandidateIdentifyResult {
                verdict: verdict_of(
                    &state.content_hash,
                    &kind,
                    state.verdict_track_count,
                    state.verdict_matched_barcode,
                    lists.remove(&state.content_hash).unwrap_or_default(),
                )?,
                probed_total_duration_ms: u64::try_from(probed).map_err(|_| {
                    DbError::Message(format!(
                        "import candidate {} holds a negative probed total",
                        state.content_hash
                    ))
                })?,
                identified_at,
            }),
            // The table writes and clears the identify columns as one group.
            _ => None,
        };
        let mut file_edits = edits.remove(&state.content_hash).unwrap_or_default();
        file_edits.revision = u64::try_from(state.edit_revision).map_err(|_| {
            DbError::Message(format!(
                "import candidate {} has a negative edit revision",
                state.content_hash
            ))
        })?;
        out.insert(
            state.content_hash.clone(),
            DbImportCandidateState {
                durations: durations.remove(&state.content_hash).unwrap_or_default(),
                signals: signals.remove(&state.content_hash),
                content_hash: state.content_hash,
                folder_path: state.folder_path,
                identify,
                file_edits,
                metadata_provenance: state.metadata_provenance,
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
