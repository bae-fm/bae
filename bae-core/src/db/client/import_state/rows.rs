//! Reading `import_candidate_state` back: the identify columns with their
//! match rows, the identity pick, and the per-file decisions.

use super::edit_rows::{apply_file_edit_row, read_file_edit_row};
use super::verdict_rows::{read_match_row, unreadable, verdict_of, MatchLists};
use super::*;
use crate::import::{ClaimLevel, IdentityPick, MetadataSource};
use std::str::FromStr;

/// The identity pick as its four columns. `kind` alone is set for the folder's
/// own tags; a pressing sets all four.
pub(super) struct PickColumns<'a> {
    pub(super) kind: &'static str,
    pub(super) source: Option<&'static str>,
    pub(super) release_id: Option<&'a str>,
    pub(super) claim: Option<&'static str>,
}

pub(super) fn pick_columns(pick: &IdentityPick) -> PickColumns<'_> {
    match pick {
        IdentityPick::Unknown => PickColumns {
            kind: "unknown",
            source: None,
            release_id: None,
            claim: None,
        },
        IdentityPick::Release {
            source,
            release_id,
            claim,
        } => PickColumns {
            kind: "release",
            source: Some(source.as_str()),
            release_id: Some(release_id.as_str()),
            claim: Some(match claim {
                ClaimLevel::Exact => "exact",
                ClaimLevel::Approximate => "approximate",
            }),
        },
    }
}

fn pick_of(
    kind: Option<String>,
    source: Option<String>,
    release_id: Option<String>,
    claim: Option<String>,
) -> Result<Option<IdentityPick>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    match kind.as_str() {
        "unknown" => Ok(Some(IdentityPick::Unknown)),
        "release" => {
            let missing = |what: &str| DbError::Message(format!("a stored pick names no {what}"));
            let claim = match claim.ok_or_else(|| missing("claim"))?.as_str() {
                "exact" => ClaimLevel::Exact,
                "approximate" => ClaimLevel::Approximate,
                other => return Err(unreadable("pick_claim", other)),
            };
            Ok(Some(IdentityPick::Release {
                source: MetadataSource::from_str(&source.ok_or_else(|| missing("source"))?)
                    .map_err(DbError::Message)?,
                release_id: release_id.ok_or_else(|| missing("release"))?,
                claim,
            }))
        }
        other => Err(unreadable("pick_kind", other)),
    }
}

struct StateRow {
    content_hash: String,
    folder_path: String,
    verdict_kind: Option<String>,
    verdict_track_count: Option<i64>,
    verdict_group_source: Option<String>,
    verdict_group_id: Option<String>,
    verdict_matched_barcode: Option<String>,
    probed_total_duration_ms: Option<i64>,
    identified_at: Option<DateTime<Utc>>,
    pick: Option<IdentityPick>,
    edit_revision: i64,
}

fn read_state_row(row: &Row<'_>) -> Result<StateRow, DbError> {
    let identified_at: Option<String> = row.get("identified_at")?;
    Ok(StateRow {
        content_hash: row.get("content_hash")?,
        folder_path: row.get("folder_path")?,
        verdict_kind: row.get("verdict_kind")?,
        verdict_track_count: row.get("verdict_track_count")?,
        verdict_group_source: row.get("verdict_group_source")?,
        verdict_group_id: row.get("verdict_group_id")?,
        verdict_matched_barcode: row.get("verdict_matched_barcode")?,
        probed_total_duration_ms: row.get("probed_total_duration_ms")?,
        identified_at: identified_at
            .map(|_| rfc3339_column(row, "identified_at"))
            .transpose()?,
        pick: pick_of(
            row.get("pick_kind")?,
            row.get("pick_source")?,
            row.get("pick_release_id")?,
            row.get("pick_claim")?,
        )?,
        edit_revision: row.get("edit_revision")?,
    })
}

const STATE_COLUMNS: &str = "content_hash, folder_path, verdict_kind, verdict_track_count, \
     verdict_group_source, verdict_group_id, verdict_matched_barcode, \
     probed_total_duration_ms, identified_at, pick_kind, pick_source, pick_release_id, \
     pick_claim, edit_revision";

const MATCH_COLUMNS: &str = "content_hash, list, source, release_id, title, artist, year, \
     format, label, catalog_number, country, cover_url, cover_thumbnail_url, cover_label, \
     cover_source, source_group_id, source_tracks_kind, source_tracks_count, \
     source_tracks_total_ms, by_disc_id, by_barcode, matches_catalog";

const FILE_EDIT_COLUMNS: &str = "content_hash, relative_path, role_choice, sheet_binding, \
     sheet_binding_file_id, sheet_disc, sheet_disc_number";

/// Every stored candidate row, or the one `only` names, assembled with the
/// match and file-decision rows that hang off it.
pub(super) fn load_states_on(
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
             ORDER BY content_hash, list, position"
        ),
        named_params! { ":only": only },
        |row| Ok(read_match_row(row)),
    )?;
    let mut lists: HashMap<String, MatchLists> = HashMap::new();
    for row in matches {
        let row = row?;
        lists
            .entry(row.content_hash.clone())
            .or_default()
            .push(row)?;
    }
    let mut edits = load_edits_on(sql, only)?;

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
                    state.verdict_group_source,
                    state.verdict_group_id,
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
                content_hash: state.content_hash,
                folder_path: state.folder_path,
                identify,
                file_edits,
                identity_pick: state.pick,
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
