//! Reading scan items back out of the rows [`super::write`] laid down.
//!
//! One query per table rather than one per candidate: a watched root holds a
//! few thousand file rows and a handful of boundaries, and the assembly below
//! groups them by the key they hang off.

use super::columns::*;
use super::write::StoredEntry;
use super::*;
use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, FolderReleaseBoundary,
    FolderReleaseCandidateSummary, FolderReleaseDecisionKey, FolderReleaseTreeRow,
    FolderReleaseTreeRowKind, InvalidCandidate, ResolvedFolderReleaseBoundary, ScanItem,
    ScannedFile,
};

/// One stored entry: the key it is addressed by, the scan generation that
/// wrote it, and the item itself.
pub(crate) struct StoredScanItem {
    pub(crate) key: String,
    pub(crate) generation: u64,
    pub(crate) item: ScanItem,
}

/// Every entry stored under one watched root, in persisted-key order.
///
/// An entry stamped with a generation the root never reached is a store
/// nothing here wrote, so the assembly refuses it.
pub(super) fn load_items(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
) -> Result<Vec<StoredScanItem>, DbError> {
    let mut items = load_candidate_items(sql, watched_folder_path, None)?;
    items.extend(load_boundary_items(sql, watched_folder_path, None)?);
    items.sort_by(|left, right| left.key.cmp(&right.key));
    let root_generation = sql
        .query(
            "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
            [watched_folder_path],
            |row| row.get::<_, i64>(0),
        )?
        .into_iter()
        .next()
        .map(|generation| to_u64(generation, "a folder scan root's generation"))
        .transpose()?;
    if let Some(root_generation) = root_generation {
        for item in &items {
            if item.generation > root_generation {
                return Err(DbError::Message(format!(
                    "folder scan entry {} has generation {} newer than root generation {}",
                    item.key, item.generation, root_generation
                )));
            }
        }
    }
    Ok(items)
}

/// The entry at `entry_key`, and the root it is under. Watched roots never
/// overlap and keys are absolute paths, so at most one root holds it.
pub(crate) fn load_item_by_key(
    sql: &(impl QueryOne + QueryRows),
    entry_key: &str,
) -> Result<Option<(String, StoredScanItem)>, DbError> {
    let roots = sql.query(
        "SELECT watched_folder_path FROM scan_candidate WHERE path = ?",
        [entry_key],
        |row| row.get::<_, String>(0),
    )?;
    if roots.len() > 1 {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} is stored under {} roots",
            roots.len()
        )));
    }
    if let Some(root) = roots.into_iter().next() {
        let mut items = load_candidate_items(sql, &root, Some(entry_key))?;
        return match items.pop() {
            Some(item) => Ok(Some((root, item))),
            None => Err(DbError::Message(format!(
                "folder scan candidate {entry_key} vanished between its two reads"
            ))),
        };
    }
    let boundaries = sql.query(
        "SELECT watched_folder_path, relative_folder_path FROM scan_boundary",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let Some((root, relative)) = boundaries
        .into_iter()
        .find(|(root, relative)| super::boundary_key(root, relative) == entry_key)
    else {
        return Ok(None);
    };
    let mut items = load_boundary_items(sql, &root, Some(&relative))?;
    match items.pop() {
        Some(item) => Ok(Some((root, item))),
        None => Err(DbError::Message(format!(
            "folder scan boundary {entry_key} vanished between its two reads"
        ))),
    }
}

/// Every stored entry under one root, as the key it is addressed by and the
/// row that holds it — what a superseding write deletes by.
/// Whether the stored candidate at `path` is a settled release row — the kind
/// the list draws and counts. `false` covers a tentative or invalid row and a
/// path nothing is stored for.
pub(crate) fn candidate_is_valid(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    path: &str,
) -> Result<bool, DbError> {
    let kind: Option<String> = sql
        .query_row(
            "SELECT kind FROM scan_candidate WHERE watched_folder_path = ? AND path = ?",
            [watched_folder_path, path],
            |row| row.get(0),
        )
        .optional()?;
    Ok(kind.as_deref() == Some("valid"))
}

pub(crate) fn stored_entries(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
) -> Result<Vec<(String, StoredEntry)>, DbError> {
    let mut entries: Vec<(String, StoredEntry)> = sql
        .query(
            "SELECT path, scope FROM scan_candidate WHERE watched_folder_path = ?",
            [watched_folder_path],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.as_deref() == Some("recursive"),
                ))
            },
        )?
        .into_iter()
        .map(|(path, whole_folder)| (path.clone(), StoredEntry::Candidate { path, whole_folder }))
        .collect();
    entries.extend(
        sql.query(
            "SELECT relative_folder_path FROM scan_boundary WHERE watched_folder_path = ?",
            [watched_folder_path],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .map(|relative_folder_path| {
            (
                super::boundary_key(watched_folder_path, &relative_folder_path),
                StoredEntry::Boundary {
                    relative_folder_path,
                },
            )
        }),
    );
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

// ── Candidates ──────────────────────────────────────────────────────────────

struct CandidateRow {
    path: String,
    generation: i64,
    kind: String,
    name: String,
    display_path: String,
    file_root: Option<String>,
    scope: Option<String>,
    file_edit_revision: i64,
    format_label: Option<String>,
    combine_ancestor_relative_path: Option<String>,
    invalid_reason: Option<String>,
    invalid_reason_path: Option<String>,
}

pub(crate) fn load_candidate_items(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<Vec<StoredScanItem>, DbError> {
    let rows = sql.query(
        "SELECT path, generation, kind, name, display_path, file_root, scope, \
                file_edit_revision, format_label, combine_ancestor_relative_path, \
                invalid_reason, invalid_reason_path \
         FROM scan_candidate \
         WHERE watched_folder_path = :root AND (:only IS NULL OR path = :only) \
         ORDER BY path",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok(CandidateRow {
                path: row.get(0)?,
                generation: row.get(1)?,
                kind: row.get(2)?,
                name: row.get(3)?,
                display_path: row.get(4)?,
                file_root: row.get(5)?,
                scope: row.get(6)?,
                file_edit_revision: row.get(7)?,
                format_label: row.get(8)?,
                combine_ancestor_relative_path: row.get(9)?,
                invalid_reason: row.get(10)?,
                invalid_reason_path: row.get(11)?,
            })
        },
    )?;
    let mut files = load_files(sql, watched_folder_path, only)?;
    let mut resolved = load_resolved_boundaries(sql, watched_folder_path, only)?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let resolved_boundaries = resolved.remove(&row.path).unwrap_or_default();
        let generation = to_u64(row.generation, "a scan candidate's generation")?;
        let item = match row.kind.as_str() {
            "invalid" => {
                let reason = row.invalid_reason.as_deref().ok_or_else(|| {
                    DbError::Message(format!("scan candidate {} has no reason", row.path))
                })?;
                ScanItem::Invalid(InvalidCandidate {
                    path: PathBuf::from(&row.path),
                    name: row.name,
                    watched_folder_path: watched_folder_path.to_string(),
                    display_path: row.display_path,
                    resolved_boundaries,
                    reason: invalid_reason_of(reason, row.invalid_reason_path)?,
                })
            }
            kind @ ("tentative" | "valid") => {
                let missing = |column: &str| {
                    DbError::Message(format!("scan candidate {} has no {column}", row.path))
                };
                let candidate = FolderCandidate {
                    path: PathBuf::from(&row.path),
                    file_root: PathBuf::from(row.file_root.ok_or_else(|| missing("file root"))?),
                    name: row.name,
                    files: CategorizedFiles {
                        files: files.remove(&row.path).unwrap_or_default(),
                        format_label: row.format_label.ok_or_else(|| missing("format label"))?,
                    },
                    watched_folder_path: watched_folder_path.to_string(),
                    scope: scope_of(&row.scope.ok_or_else(|| missing("scope"))?)?,
                    file_edit_revision: to_u64(
                        row.file_edit_revision,
                        "a scan candidate's file edit revision",
                    )?,
                    display_path: row.display_path,
                    resolved_boundaries,
                    combine_ancestor_key: row.combine_ancestor_relative_path.map(|relative| {
                        FolderReleaseDecisionKey {
                            watched_folder_path: watched_folder_path.to_string(),
                            relative_folder_path: relative,
                        }
                    }),
                };
                match kind {
                    "tentative" => ScanItem::Discovered(candidate),
                    _ => ScanItem::Valid(candidate),
                }
            }
            other => return Err(unreadable("kind", other)),
        };
        items.push(StoredScanItem {
            key: row.path,
            generation,
            item,
        });
    }
    Ok(items)
}

struct FileRow {
    candidate_path: String,
    relative_path: String,
    absolute_path: String,
    size: i64,
    file_name: String,
    dir_prefix: Option<String>,
    proposed_audio: bool,
    role: String,
    sheet_binding: Option<String>,
    sheet_binding_file_id: Option<String>,
    sheet_binding_codec: Option<String>,
    sheet_disc: Option<String>,
    sheet_disc_number: Option<i64>,
}

fn load_files(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<CandidateFile>>, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, relative_path, absolute_path, size, file_name, dir_prefix, \
                proposed_audio, role, sheet_binding, sheet_binding_file_id, \
                sheet_binding_codec, sheet_disc, sheet_disc_number \
         FROM scan_candidate_file \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok(FileRow {
                candidate_path: row.get(0)?,
                relative_path: row.get(1)?,
                absolute_path: row.get(2)?,
                size: row.get(3)?,
                file_name: row.get(4)?,
                dir_prefix: row.get(5)?,
                proposed_audio: row.get(6)?,
                role: row.get(7)?,
                sheet_binding: row.get(8)?,
                sheet_binding_file_id: row.get(9)?,
                sheet_binding_codec: row.get(10)?,
                sheet_disc: row.get(11)?,
                sheet_disc_number: row.get(12)?,
            })
        },
    )?;
    let mut sheets = load_cue_sheets(sql, watched_folder_path, only)?;
    let mut files: HashMap<String, Vec<CandidateFile>> = HashMap::new();
    for row in rows {
        let role = match row.role.as_str() {
            "audio" => FileRole::Audio,
            "artwork" => FileRole::Artwork,
            "document" => FileRole::Document,
            "other" => FileRole::Other,
            "track_sheet" => {
                let missing = |what: &str| {
                    DbError::Message(format!(
                        "track sheet {} under {} has no {what}",
                        row.relative_path, row.candidate_path
                    ))
                };
                FileRole::TrackSheet {
                    sheet: sheets
                        .remove(&SheetKey {
                            candidate_path: row.candidate_path.clone(),
                            sheet_relative_path: row.relative_path.clone(),
                        })
                        .ok_or_else(|| missing("parsed sheet"))?,
                    binding: sheet_binding_of(
                        &row.sheet_binding.ok_or_else(|| missing("binding"))?,
                        row.sheet_binding_file_id,
                        row.sheet_binding_codec,
                    )?,
                    disc: sheet_disc_of(
                        &row.sheet_disc.ok_or_else(|| missing("disc"))?,
                        row.sheet_disc_number,
                    )?,
                }
            }
            other => return Err(unreadable("role", other)),
        };
        files
            .entry(row.candidate_path)
            .or_default()
            .push(CandidateFile {
                file: ScannedFile {
                    path: PathBuf::from(row.absolute_path),
                    relative_path: row.relative_path,
                    size: to_u64(row.size, "a file's size")?,
                    dir_prefix: row.dir_prefix,
                    file_name: row.file_name,
                },
                role,
                proposed_audio: row.proposed_audio,
            });
    }
    Ok(files)
}

pub(crate) fn load_resolved_boundaries(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<ResolvedFolderReleaseBoundary>>, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, relative_folder_path, decision, name, display_path \
         FROM scan_candidate_resolved_boundary \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?;
    let mut resolved: HashMap<String, Vec<ResolvedFolderReleaseBoundary>> = HashMap::new();
    for (candidate_path, relative_folder_path, decision, name, display_path) in rows {
        resolved
            .entry(candidate_path)
            .or_default()
            .push(ResolvedFolderReleaseBoundary {
                key: FolderReleaseDecisionKey {
                    watched_folder_path: watched_folder_path.to_string(),
                    relative_folder_path,
                },
                decision: decision_of(&decision)?,
                name,
                display_path,
            });
    }
    Ok(resolved)
}

// ── Track sheets ────────────────────────────────────────────────────────────

/// One parsed sheet: the candidate it sits under, and its path within it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SheetKey {
    candidate_path: String,
    sheet_relative_path: String,
}

/// One track of one sheet, by its position in that sheet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TrackKey {
    sheet: SheetKey,
    position: i64,
}

fn load_cue_sheets(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<SheetKey, CueSheet>, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, title, performer, catalog, date \
         FROM scan_cue_sheet \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only)",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                CueSheet {
                    title: row.get(2)?,
                    performer: row.get(3)?,
                    catalog: row.get(4)?,
                    date: row.get(5)?,
                    tracks: Vec::new(),
                },
            ))
        },
    )?;
    let mut tracks = load_cue_tracks(sql, watched_folder_path, only)?;
    let mut sheets = HashMap::with_capacity(rows.len());
    for (candidate_path, sheet_relative_path, mut sheet) in rows {
        let key = SheetKey {
            candidate_path,
            sheet_relative_path,
        };
        sheet.tracks = tracks.remove(&key).unwrap_or_default();
        sheets.insert(key, sheet);
    }
    Ok(sheets)
}

struct CueTrackRow {
    candidate_path: String,
    sheet_relative_path: String,
    position: i64,
    number: i64,
    mode: String,
    mode_other: Option<String>,
    title: Option<String>,
    performer: Option<String>,
    file_reference: String,
    start_cue_frames: i64,
    end_cue_frames: Option<i64>,
    pregap_kind: String,
    pregap_frames: Option<i64>,
    pregap_index_number: Option<i64>,
    pregap_index_file_reference: Option<String>,
}

fn load_cue_tracks(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<SheetKey, Vec<CueTrack>>, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, position, number, mode, mode_other, \
                title, performer, file_reference, start_cue_frames, end_cue_frames, \
                pregap_kind, pregap_frames, pregap_index_number, pregap_index_file_reference \
         FROM scan_cue_track \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, sheet_relative_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok(CueTrackRow {
                candidate_path: row.get(0)?,
                sheet_relative_path: row.get(1)?,
                position: row.get(2)?,
                number: row.get(3)?,
                mode: row.get(4)?,
                mode_other: row.get(5)?,
                title: row.get(6)?,
                performer: row.get(7)?,
                file_reference: row.get(8)?,
                start_cue_frames: row.get(9)?,
                end_cue_frames: row.get(10)?,
                pregap_kind: row.get(11)?,
                pregap_frames: row.get(12)?,
                pregap_index_number: row.get(13)?,
                pregap_index_file_reference: row.get(14)?,
            })
        },
    )?;
    let mut indexes = load_cue_indexes(sql, watched_folder_path, only)?;
    let mut tracks: HashMap<SheetKey, Vec<CueTrack>> = HashMap::new();
    for row in rows {
        let pregap = match row.pregap_kind.as_str() {
            "none" => CuePregap::None,
            "silence" => CuePregap::Silence {
                frames: to_u64(
                    row.pregap_frames.ok_or_else(|| {
                        DbError::Message("a silent pregap states no length".to_string())
                    })?,
                    "a generated pregap's length",
                )?,
            },
            "audio" => {
                let missing =
                    |what: &str| DbError::Message(format!("an audio pregap states no {what}"));
                CuePregap::Audio(CueIndex {
                    number: to_u32(
                        row.pregap_index_number.ok_or_else(|| missing("index"))?,
                        "a pregap's index number",
                    )?,
                    frames: to_u64(
                        row.pregap_frames.ok_or_else(|| missing("position"))?,
                        "a pregap's frame position",
                    )?,
                    file_reference: row
                        .pregap_index_file_reference
                        .ok_or_else(|| missing("file"))?,
                })
            }
            other => return Err(unreadable("pregap_kind", other)),
        };
        let sheet = SheetKey {
            candidate_path: row.candidate_path,
            sheet_relative_path: row.sheet_relative_path,
        };
        let indexes = indexes
            .remove(&TrackKey {
                sheet: sheet.clone(),
                position: row.position,
            })
            .unwrap_or_default();
        tracks.entry(sheet).or_default().push(CueTrack {
            number: to_u32(row.number, "a cue track's number")?,
            mode: match row.mode.as_str() {
                "audio" => CueTrackMode::Audio,
                "other" => CueTrackMode::Other(row.mode_other.ok_or_else(|| {
                    DbError::Message("a non-audio cue track names no mode".to_string())
                })?),
                other => return Err(unreadable("mode", other)),
            },
            title: row.title,
            performer: row.performer,
            indexes,
            file_reference: row.file_reference,
            start_cue_frames: to_u64(row.start_cue_frames, "a cue track's start")?,
            pregap,
            end_cue_frames: row
                .end_cue_frames
                .map(|frames| to_u64(frames, "a cue track's end"))
                .transpose()?,
        });
    }
    Ok(tracks)
}

fn load_cue_indexes(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<TrackKey, Vec<CueIndex>>, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, track_position, number, frames, \
                file_reference \
         FROM scan_cue_index \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, sheet_relative_path, track_position, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )?;
    let mut indexes: HashMap<TrackKey, Vec<CueIndex>> = HashMap::new();
    for (candidate_path, sheet_relative_path, track_position, number, frames, file_reference) in
        rows
    {
        indexes
            .entry(TrackKey {
                sheet: SheetKey {
                    candidate_path,
                    sheet_relative_path,
                },
                position: track_position,
            })
            .or_default()
            .push(CueIndex {
                number: to_u32(number, "a cue index's number")?,
                frames: to_u64(frames, "a cue index's frame position")?,
                file_reference,
            });
    }
    Ok(indexes)
}

// ── Boundaries ──────────────────────────────────────────────────────────────

struct TreeRow {
    boundary: String,
    position: i64,
    name: String,
    display_path: String,
    depth: i64,
    kind: String,
    track_count: Option<i64>,
    format_label: Option<String>,
    invalid_reason: Option<String>,
    invalid_reason_path: Option<String>,
    decision_relative_folder_path: String,
}

pub(crate) fn load_boundary_items(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<Vec<StoredScanItem>, DbError> {
    let rows = sql.query(
        "SELECT relative_folder_path, generation, name, display_path, shared_file_count \
         FROM scan_boundary \
         WHERE watched_folder_path = :root \
           AND (:only IS NULL OR relative_folder_path = :only) \
         ORDER BY relative_folder_path",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;
    let mut tree_rows = load_tree_rows(sql, watched_folder_path, only)?;
    let mut hidden = load_hidden_candidates(sql, watched_folder_path, only)?;
    let mut items = Vec::with_capacity(rows.len());
    for (relative_folder_path, generation, name, display_path, shared_file_count) in rows {
        let key = super::boundary_key(watched_folder_path, &relative_folder_path);
        items.push(StoredScanItem {
            key,
            generation: to_u64(generation, "a scan boundary's generation")?,
            item: ScanItem::Boundary(FolderReleaseBoundary {
                tree_rows: tree_rows.remove(&relative_folder_path).unwrap_or_default(),
                candidate_keys: hidden.remove(&relative_folder_path).unwrap_or_default(),
                key: FolderReleaseDecisionKey {
                    watched_folder_path: watched_folder_path.to_string(),
                    relative_folder_path,
                },
                name,
                display_path,
                shared_file_count: to_u32(shared_file_count, "a boundary's shared file count")?,
            }),
        });
    }
    Ok(items)
}

fn load_tree_rows(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<FolderReleaseTreeRow>>, DbError> {
    let rows = sql.query(
        "SELECT boundary_relative_folder_path, position, name, display_path, depth, kind, \
                track_count, format_label, invalid_reason, invalid_reason_path, \
                decision_relative_folder_path \
         FROM scan_boundary_tree_row \
         WHERE watched_folder_path = :root \
           AND (:only IS NULL OR boundary_relative_folder_path = :only) \
         ORDER BY boundary_relative_folder_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok(TreeRow {
                boundary: row.get(0)?,
                position: row.get(1)?,
                name: row.get(2)?,
                display_path: row.get(3)?,
                depth: row.get(4)?,
                kind: row.get(5)?,
                track_count: row.get(6)?,
                format_label: row.get(7)?,
                invalid_reason: row.get(8)?,
                invalid_reason_path: row.get(9)?,
                decision_relative_folder_path: row.get(10)?,
            })
        },
    )?;
    let mut ancestors = load_tree_row_ancestors(sql, watched_folder_path, only)?;
    let mut tree_rows: HashMap<String, Vec<FolderReleaseTreeRow>> = HashMap::new();
    for row in rows {
        let missing = |what: &str| {
            DbError::Message(format!(
                "boundary tree row {} of {} has no {what}",
                row.position, row.boundary
            ))
        };
        let kind = match row.kind.as_str() {
            "folder" => FolderReleaseTreeRowKind::Folder,
            "candidate" => FolderReleaseTreeRowKind::Candidate {
                summary: FolderReleaseCandidateSummary {
                    track_count: to_u32(
                        row.track_count.ok_or_else(|| missing("track count"))?,
                        "a boundary row's track count",
                    )?,
                    format_label: row.format_label.ok_or_else(|| missing("format label"))?,
                },
            },
            "invalid" => FolderReleaseTreeRowKind::Invalid {
                reason: invalid_reason_of(
                    &row.invalid_reason.ok_or_else(|| missing("reason"))?,
                    row.invalid_reason_path,
                )?,
            },
            other => return Err(unreadable("kind", other)),
        };
        let ancestor_decision_keys = ancestors
            .remove(&(row.boundary.clone(), row.position))
            .unwrap_or_default()
            .into_iter()
            .map(|relative_folder_path| FolderReleaseDecisionKey {
                watched_folder_path: watched_folder_path.to_string(),
                relative_folder_path,
            })
            .collect();
        tree_rows
            .entry(row.boundary)
            .or_default()
            .push(FolderReleaseTreeRow {
                name: row.name,
                display_path: row.display_path,
                depth: to_u32(row.depth, "a boundary row's depth")?,
                kind,
                decision_key: FolderReleaseDecisionKey {
                    watched_folder_path: watched_folder_path.to_string(),
                    relative_folder_path: row.decision_relative_folder_path,
                },
                ancestor_decision_keys,
            });
    }
    Ok(tree_rows)
}

fn load_tree_row_ancestors(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<(String, i64), Vec<String>>, DbError> {
    let rows = sql.query(
        "SELECT boundary_relative_folder_path, row_position, ancestor_relative_folder_path \
         FROM scan_boundary_tree_row_ancestor \
         WHERE watched_folder_path = :root \
           AND (:only IS NULL OR boundary_relative_folder_path = :only) \
         ORDER BY boundary_relative_folder_path, row_position, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let mut ancestors: HashMap<(String, i64), Vec<String>> = HashMap::new();
    for (boundary, row_position, ancestor) in rows {
        ancestors
            .entry((boundary, row_position))
            .or_default()
            .push(ancestor);
    }
    Ok(ancestors)
}

fn load_hidden_candidates(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<String>>, DbError> {
    let rows = sql.query(
        "SELECT boundary_relative_folder_path, candidate_path \
         FROM scan_boundary_hidden_candidate \
         WHERE watched_folder_path = :root \
           AND (:only IS NULL OR boundary_relative_folder_path = :only) \
         ORDER BY boundary_relative_folder_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut hidden: HashMap<String, Vec<String>> = HashMap::new();
    for (boundary, candidate_path) in rows {
        hidden.entry(boundary).or_default().push(candidate_path);
    }
    Ok(hidden)
}
