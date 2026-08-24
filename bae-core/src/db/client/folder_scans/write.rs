//! Writing one scan item down as rows. A saved item replaces what was stored
//! under its key: the candidate or boundary row is deleted first, and every
//! table below it goes with it by cascade, so the insert that follows is
//! always writing into empty space.

use super::columns::*;
use super::*;
use crate::cue_flac::{CuePregap, CueSheet};
use crate::import::folder_scanner::{
    CandidateFile, FileRole, FolderCandidate, FolderReleaseBoundary, FolderReleaseTreeRowKind,
    InvalidCandidate, ResolvedFolderReleaseBoundary, ScanItem,
};

/// Where one stored entry lives, so the writer that supersedes it can name
/// the row exactly rather than reconstructing a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoredEntry {
    Candidate {
        path: String,
        /// Whether this candidate's files are the whole folder at `path` —
        /// the shape a combined folder stores as.
        whole_folder: bool,
    },
    Boundary {
        relative_folder_path: String,
    },
}

pub(crate) fn delete_entry(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    entry: &StoredEntry,
) -> Result<(), DbError> {
    match entry {
        StoredEntry::Candidate { path, .. } => {
            sql.execute(
                "DELETE FROM scan_candidate WHERE watched_folder_path = ? AND path = ?",
                params![watched_folder_path, path],
            )?;
        }
        StoredEntry::Boundary {
            relative_folder_path,
        } => {
            sql.execute(
                "DELETE FROM scan_boundary \
                 WHERE watched_folder_path = ? AND relative_folder_path = ?",
                params![watched_folder_path, relative_folder_path],
            )?;
        }
    }
    Ok(())
}

/// Delete every row not written in `generation`, and say which keys went.
pub(super) fn prune_other_generations(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
) -> Result<Vec<String>, DbError> {
    let mut pruned: Vec<String> = sql.query(
        "SELECT path FROM scan_candidate \
         WHERE watched_folder_path = ? AND generation != ?",
        params![watched_folder_path, generation],
        |row| row.get::<_, String>(0),
    )?;
    for relative_folder_path in sql.query(
        "SELECT relative_folder_path FROM scan_boundary \
         WHERE watched_folder_path = ? AND generation != ?",
        params![watched_folder_path, generation],
        |row| row.get::<_, String>(0),
    )? {
        pruned.push(super::boundary_key(
            watched_folder_path,
            &relative_folder_path,
        ));
    }
    sql.execute(
        "DELETE FROM scan_candidate WHERE watched_folder_path = ? AND generation != ?",
        params![watched_folder_path, generation],
    )?;
    sql.execute(
        "DELETE FROM scan_boundary WHERE watched_folder_path = ? AND generation != ?",
        params![watched_folder_path, generation],
    )?;
    pruned.sort();
    Ok(pruned)
}

/// Stamp an existing candidate row with `generation`, leaving its content
/// alone, so the completion prune counts it as seen by this scan.
pub(crate) fn touch_candidate(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    path: &str,
    generation: i64,
) -> Result<(), DbError> {
    sql.execute(
        "UPDATE scan_candidate SET generation = ? WHERE watched_folder_path = ? AND path = ?",
        params![generation, watched_folder_path, path],
    )?;
    Ok(())
}

/// Stamp the stored boundary at this key with the current generation, so a
/// pass that found it unchanged keeps it through the completion prune.
pub(crate) fn touch_boundary(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    relative_folder_path: &str,
    generation: i64,
) -> Result<(), DbError> {
    sql.execute(
        "UPDATE scan_boundary SET generation = ? \
         WHERE watched_folder_path = ? AND relative_folder_path = ?",
        params![generation, watched_folder_path, relative_folder_path],
    )?;
    Ok(())
}

pub(super) fn insert_item(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    item: &ScanItem,
) -> Result<(), DbError> {
    match item {
        ScanItem::Discovered(candidate) => {
            insert_candidate(sql, watched_folder_path, generation, "tentative", candidate)
        }
        ScanItem::Valid(candidate) => {
            insert_candidate(sql, watched_folder_path, generation, "valid", candidate)
        }
        ScanItem::Invalid(candidate) => {
            insert_invalid(sql, watched_folder_path, generation, candidate)
        }
        ScanItem::Boundary(boundary) => {
            insert_boundary(sql, watched_folder_path, generation, boundary)
        }
        ScanItem::Decided { .. } => Err(DbError::Message(
            "a folder reading is stored as a decision, not as a scan entry".to_string(),
        )),
    }
}

fn insert_candidate(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    kind: &str,
    candidate: &FolderCandidate,
) -> Result<(), DbError> {
    let path = candidate.path.to_string_lossy().into_owned();
    sql.execute(
        "INSERT INTO scan_candidate \
             (watched_folder_path, path, generation, kind, name, display_path, file_root, \
              scope, content_hash, file_edit_revision, format_label, \
              combine_ancestor_relative_path, invalid_reason, invalid_reason_path) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)",
        params![
            watched_folder_path,
            path,
            generation,
            kind,
            candidate.name,
            candidate.display_path,
            candidate.file_root.to_string_lossy(),
            scope_text(candidate.scope),
            candidate.files.content_hash(),
            to_i64(
                candidate.file_edit_revision,
                "a candidate's file edit revision"
            )?,
            candidate.files.format_label,
            candidate
                .combine_ancestor_key
                .as_ref()
                .map(|key| key.relative_folder_path.as_str()),
        ],
    )?;
    insert_candidate_files(sql, watched_folder_path, &path, &candidate.files)?;
    insert_resolved_boundaries(
        sql,
        watched_folder_path,
        &path,
        &candidate.resolved_boundaries,
    )
}

fn insert_invalid(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    candidate: &InvalidCandidate,
) -> Result<(), DbError> {
    let path = candidate.path.to_string_lossy().into_owned();
    let (reason, reason_path) = invalid_reason_columns(&candidate.reason);
    sql.execute(
        "INSERT INTO scan_candidate \
             (watched_folder_path, path, generation, kind, name, display_path, file_root, \
              scope, content_hash, file_edit_revision, format_label, \
              combine_ancestor_relative_path, invalid_reason, invalid_reason_path) \
         VALUES (?, ?, ?, 'invalid', ?, ?, NULL, NULL, NULL, 0, NULL, NULL, ?, ?)",
        params![
            watched_folder_path,
            path,
            generation,
            candidate.name,
            candidate.display_path,
            reason,
            reason_path,
        ],
    )?;
    insert_resolved_boundaries(
        sql,
        watched_folder_path,
        &path,
        &candidate.resolved_boundaries,
    )
}

/// Lay down one candidate's files, their parsed track sheets and all. Also
/// the write a file decision makes: the settled shape replaces the rows the
/// scan proposed, under the same candidate.
pub(crate) fn insert_candidate_files(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    files: &crate::import::folder_scanner::CategorizedFiles,
) -> Result<(), DbError> {
    for (position, file) in files.files.iter().enumerate() {
        insert_file(sql, watched_folder_path, candidate_path, position, file)?;
    }
    Ok(())
}

fn insert_file(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    position: usize,
    file: &CandidateFile,
) -> Result<(), DbError> {
    let columns = role_columns(&file.role);
    sql.execute(
        "INSERT INTO scan_candidate_file \
             (watched_folder_path, candidate_path, relative_path, position, absolute_path, \
              size, file_name, dir_prefix, proposed_audio, role, sheet_binding, \
              sheet_binding_file_id, sheet_binding_codec, sheet_disc, sheet_disc_number) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            watched_folder_path,
            candidate_path,
            file.file.relative_path,
            to_i64(position as u64, "a candidate file's position")?,
            file.file.path.to_string_lossy(),
            to_i64(file.file.size, "a file's size")?,
            file.file.file_name,
            file.file.dir_prefix,
            file.proposed_audio,
            columns.role,
            columns.sheet_binding,
            columns.sheet_binding_file_id,
            columns.sheet_binding_codec,
            columns.sheet_disc,
            columns.sheet_disc_number,
        ],
    )?;
    if let FileRole::TrackSheet { sheet, .. } = &file.role {
        insert_cue_sheet(
            sql,
            watched_folder_path,
            candidate_path,
            &file.file.relative_path,
            sheet,
        )?;
    }
    Ok(())
}

fn insert_cue_sheet(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    sheet_relative_path: &str,
    sheet: &CueSheet,
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO scan_cue_sheet \
             (watched_folder_path, candidate_path, sheet_relative_path, title, performer, \
              catalog, date) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            watched_folder_path,
            candidate_path,
            sheet_relative_path,
            sheet.title,
            sheet.performer,
            sheet.catalog,
            sheet.date,
        ],
    )?;
    for (position, track) in sheet.tracks.iter().enumerate() {
        let position = to_i64(position as u64, "a cue track's position")?;
        let (mode, mode_other) = match &track.mode {
            crate::cue_flac::CueTrackMode::Audio => ("audio", None),
            crate::cue_flac::CueTrackMode::Other(other) => ("other", Some(other.as_str())),
        };
        let (pregap_kind, pregap_frames, pregap_index_number, pregap_index_file_reference) =
            match &track.pregap {
                CuePregap::None => ("none", None, None, None),
                CuePregap::Audio(index) => (
                    "audio",
                    Some(to_i64(index.frames, "a pregap's frame position")?),
                    Some(index.number),
                    Some(index.file_reference.as_str()),
                ),
                CuePregap::Silence { frames } => (
                    "silence",
                    Some(to_i64(*frames, "a generated pregap's length")?),
                    None,
                    None,
                ),
            };
        sql.execute(
            "INSERT INTO scan_cue_track \
                 (watched_folder_path, candidate_path, sheet_relative_path, position, number, \
                  mode, mode_other, title, performer, file_reference, start_cue_frames, \
                  end_cue_frames, pregap_kind, pregap_frames, pregap_index_number, \
                  pregap_index_file_reference) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                watched_folder_path,
                candidate_path,
                sheet_relative_path,
                position,
                track.number,
                mode,
                mode_other,
                track.title,
                track.performer,
                track.file_reference,
                to_i64(track.start_cue_frames, "a cue track's start")?,
                track
                    .end_cue_frames
                    .map(|frames| to_i64(frames, "a cue track's end"))
                    .transpose()?,
                pregap_kind,
                pregap_frames,
                pregap_index_number,
                pregap_index_file_reference,
            ],
        )?;
        for (index_position, index) in track.indexes.iter().enumerate() {
            sql.execute(
                "INSERT INTO scan_cue_index \
                     (watched_folder_path, candidate_path, sheet_relative_path, track_position, \
                      position, number, frames, file_reference) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    watched_folder_path,
                    candidate_path,
                    sheet_relative_path,
                    position,
                    to_i64(index_position as u64, "a cue index's position")?,
                    index.number,
                    to_i64(index.frames, "a cue index's frame position")?,
                    index.file_reference,
                ],
            )?;
        }
    }
    Ok(())
}

fn insert_resolved_boundaries(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    resolved: &[ResolvedFolderReleaseBoundary],
) -> Result<(), DbError> {
    for (position, boundary) in resolved.iter().enumerate() {
        sql.execute(
            "INSERT INTO scan_candidate_resolved_boundary \
                 (watched_folder_path, candidate_path, position, relative_folder_path, \
                  decision, name, display_path) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                watched_folder_path,
                candidate_path,
                to_i64(position as u64, "a resolved boundary's position")?,
                boundary.key.relative_folder_path,
                decision_text(boundary.decision),
                boundary.name,
                boundary.display_path,
            ],
        )?;
    }
    Ok(())
}

fn insert_boundary(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    boundary: &FolderReleaseBoundary,
) -> Result<(), DbError> {
    let relative = boundary.key.relative_folder_path.as_str();
    sql.execute(
        "INSERT INTO scan_boundary \
             (watched_folder_path, relative_folder_path, generation, name, display_path, \
              shared_file_count) \
         VALUES (?, ?, ?, ?, ?, ?)",
        params![
            watched_folder_path,
            relative,
            generation,
            boundary.name,
            boundary.display_path,
            boundary.shared_file_count,
        ],
    )?;
    for (position, row) in boundary.tree_rows.iter().enumerate() {
        let position = to_i64(position as u64, "a boundary tree row's position")?;
        let (kind, track_count, format_label) = match &row.kind {
            FolderReleaseTreeRowKind::Folder => ("folder", None, None),
            FolderReleaseTreeRowKind::Candidate { summary } => (
                "candidate",
                Some(summary.track_count),
                Some(summary.format_label.as_str()),
            ),
            FolderReleaseTreeRowKind::Invalid { .. } => ("invalid", None, None),
        };
        let (invalid_reason, invalid_reason_path) = match &row.kind {
            FolderReleaseTreeRowKind::Invalid { reason } => {
                let (reason, path) = invalid_reason_columns(reason);
                (Some(reason), path)
            }
            FolderReleaseTreeRowKind::Folder | FolderReleaseTreeRowKind::Candidate { .. } => {
                (None, None)
            }
        };
        sql.execute(
            "INSERT INTO scan_boundary_tree_row \
                 (watched_folder_path, boundary_relative_folder_path, position, name, \
                  display_path, depth, kind, track_count, format_label, invalid_reason, \
                  invalid_reason_path, decision_relative_folder_path) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                watched_folder_path,
                relative,
                position,
                row.name,
                row.display_path,
                row.depth,
                kind,
                track_count,
                format_label,
                invalid_reason,
                invalid_reason_path,
                row.decision_key.relative_folder_path,
            ],
        )?;
        for (ancestor_position, ancestor) in row.ancestor_decision_keys.iter().enumerate() {
            sql.execute(
                "INSERT INTO scan_boundary_tree_row_ancestor \
                     (watched_folder_path, boundary_relative_folder_path, row_position, \
                      position, ancestor_relative_folder_path) \
                 VALUES (?, ?, ?, ?, ?)",
                params![
                    watched_folder_path,
                    relative,
                    position,
                    to_i64(ancestor_position as u64, "an ancestor key's position")?,
                    ancestor.relative_folder_path,
                ],
            )?;
        }
    }
    for (position, candidate_path) in boundary.candidate_keys.iter().enumerate() {
        sql.execute(
            "INSERT INTO scan_boundary_hidden_candidate \
                 (watched_folder_path, boundary_relative_folder_path, position, candidate_path) \
             VALUES (?, ?, ?, ?)",
            params![
                watched_folder_path,
                relative,
                to_i64(position as u64, "a hidden candidate's position")?,
                candidate_path,
            ],
        )?;
    }
    Ok(())
}
