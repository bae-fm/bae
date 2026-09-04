//! Reading `import_candidate_state` back: normal verdict columns or an attached
//! failed verdict, match rows, metadata draft and provenance, and file decisions.

use super::edit_rows::{apply_file_edit_row, read_file_edit_row};
use super::signal_rows::load_signals_on;
use super::verdict_rows::{
    read_identify_failure_row, read_match_row, unreadable, verdict_of, IdentifyFailureRow,
    MatchLists,
};
use super::*;
use crate::import::{MetadataProvenance, MetadataRef, MetadataSource};
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
        MetadataProvenance::ExternalRelease {
            source, release_id, ..
        } => SeedColumns {
            kind: "external_release",
            source: Some(source.as_str()),
            release_id: Some(release_id.as_str()),
        },
    }
}

/// The provenance columns plus the partner rows the same write laid down.
/// `partners` is ignored for anything but an external release, which is the
/// only provenance that can carry them.
pub(crate) fn metadata_provenance_of(
    kind: Option<String>,
    source: Option<String>,
    release_id: Option<String>,
    partners: Vec<MetadataRef>,
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
                partners,
            }))
        }
        other => Err(unreadable("provenance_kind", other)),
    }
}

/// Replace the partner rows of one candidate to match `provenance`, in the
/// transaction that writes its provenance columns. A provenance carrying none —
/// File Tags, a cleared draft, a pick from one source — leaves the table empty
/// for that candidate rather than keeping rows the columns no longer explain.
pub(super) fn replace_provenance_partners(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    provenance: Option<&MetadataProvenance>,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_provenance_partner WHERE content_hash = ?",
        params![content_hash],
    )?;
    let Some(MetadataProvenance::ExternalRelease { partners, .. }) = provenance else {
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

/// Every candidate's partner rows, or the one `only` names, keyed by content
/// hash and ordered by source so a read back is stable.
pub(crate) fn load_provenance_partners_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<MetadataRef>>, DbError> {
    let rows = sql.query(
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
    )?;
    let mut partners: HashMap<String, Vec<MetadataRef>> = HashMap::new();
    for (content_hash, source, release_id) in rows {
        let source = MetadataSource::from_str(&source).map_err(DbError::Message)?;
        partners
            .entry(content_hash)
            .or_default()
            .push(MetadataRef::new(release_id, source));
    }
    Ok(partners)
}

struct StateRow {
    content_hash: String,
    folder_path: String,
    verdict_kind: Option<String>,
    verdict_track_count: Option<i64>,
    verdict_matched_barcode: Option<String>,
    probed_total_duration_ms: Option<i64>,
    identified_at: Option<DateTime<Utc>>,
    provenance_kind: Option<String>,
    provenance_source: Option<String>,
    provenance_release_id: Option<String>,
    edit_revision: i64,
    metadata_revision: i64,
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
        provenance_kind: row.get("provenance_kind")?,
        provenance_source: row.get("provenance_source")?,
        provenance_release_id: row.get("provenance_release_id")?,
        edit_revision: row.get("edit_revision")?,
        metadata_revision: row.get("metadata_revision")?,
    })
}

const STATE_COLUMNS: &str = "content_hash, folder_path, verdict_kind, verdict_track_count, \
     verdict_matched_barcode, probed_total_duration_ms, identified_at, provenance_kind, \
     provenance_source, provenance_release_id, edit_revision, metadata_revision";

const MATCH_COLUMNS: &str = "content_hash, source, release_id, title, artist, year, \
     format, label, catalog_number, country, barcode, cover_url, cover_thumbnail_url, \
     cover_label, cover_source, source_group_id, source_tracks_kind, source_tracks_count, \
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
    let mut signals = load_signals_on(sql, only)?;
    let mut partners = load_provenance_partners_on(sql, only)?;
    let failure_rows = sql.query(
        "SELECT content_hash, failures_json, track_count, probed_total_duration_ms, identified_at \
         FROM import_candidate_identify_failure \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| Ok(read_identify_failure_row(row)),
    )?;
    let mut failures: HashMap<String, IdentifyFailureRow> = HashMap::new();
    for row in failure_rows {
        let row = row?;
        if failures.insert(row.content_hash.clone(), row).is_some() {
            return Err(DbError::Message("duplicate identify failure row".into()));
        }
    }

    let mut out = HashMap::with_capacity(states.len());
    for state in states {
        let state = state?;
        let normal_identify = match (
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
        let failed_identify =
            failures
                .remove(&state.content_hash)
                .map(|failed| DbCandidateIdentifyResult {
                    verdict: crate::identify::TerminalVerdict::Failed {
                        failures: failed.failures,
                        track_count: failed.track_count,
                    },
                    probed_total_duration_ms: failed.probed_total_duration_ms,
                    identified_at: failed.identified_at,
                });
        let identify = match (normal_identify, failed_identify) {
            (Some(_), Some(_)) => {
                return Err(DbError::Message(format!(
                    "import candidate {} stores both a verdict and an identify failure",
                    state.content_hash
                )))
            }
            (normal, failed) => normal.or(failed),
        };
        let metadata_provenance = metadata_provenance_of(
            state.provenance_kind,
            state.provenance_source,
            state.provenance_release_id,
            partners.remove(&state.content_hash).unwrap_or_default(),
        )?;
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
                content_hash: state.content_hash,
                folder_path: state.folder_path,
                identify,
                file_edits,
                metadata_provenance,
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
