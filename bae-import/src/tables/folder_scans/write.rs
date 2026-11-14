//! Writing one scan item as rows. An item written again keeps its
//! `scan_candidate` row, which the write updates in place, and clears every
//! table below it first ([`clear_entry_rows`]), so these inserts always write
//! into empty space while what the import list holds against the row (its
//! place, whether it is selected, its group's fold) stays with it.

use super::columns::*;
use super::*;
use crate::cue_sheet::CueSheet;
use crate::folder_scanner::{CandidateFile, FileRole, FolderCandidate, InvalidCandidate, ScanItem};
use crate::CandidateKey;

/// Remove the entry at `key`: its candidate leaves the scan, and its
/// identification ends with it.
pub(crate) fn delete_entry(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    key: &CandidateKey,
) -> Result<(), DbError> {
    delete_entry_row(sql, watched_folder_path, key)?;
    crate::tables::import_state::end_file_work_unless_of_on(sql, key, None)
}

fn delete_entry_row(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    key: &CandidateKey,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM scan_candidate WHERE watched_folder_path = ? AND path = ?",
        params![coven::path_bytes(watched_folder_path), key],
    )?;
    Ok(())
}

/// Every table that hangs off a `scan_candidate` row by foreign key: what an
/// entry written again clears before its rows are written anew.
pub(super) const ENTRY_ROW_TABLES: [&str; 6] = [
    "scan_candidate_tag_snapshot",
    "scan_candidate_unit",
    "scan_candidate_part",
    "scan_candidate_file",
    "scan_skipped_file",
    "scan_skipped_tags",
];

/// Clear every row below the entry's `scan_candidate` row, for a write that
/// stores the entry again over that row and says itself what that does to
/// its candidate's identification.
pub(super) fn clear_entry_rows(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    key: &CandidateKey,
) -> Result<(), DbError> {
    for table in ENTRY_ROW_TABLES {
        sql.execute(
            &format!("DELETE FROM {table} WHERE watched_folder_path = ? AND candidate_path = ?"),
            params![coven::path_bytes(watched_folder_path), key],
        )?;
    }
    Ok(())
}

/// The clause that makes a `scan_candidate` insert write over a row already
/// stored under the same key, column for column.
const OVER_STORED_ROW: &str = "ON CONFLICT (watched_folder_path, path) DO UPDATE SET \
     generation = excluded.generation, kind = excluded.kind, name = excluded.name, \
     relative_folder = excluded.relative_folder, folder = excluded.folder, \
     file_root = excluded.file_root, scope = excluded.scope, \
     content_hash = excluded.content_hash, file_edit_revision = excluded.file_edit_revision, \
     grouping_key = excluded.grouping_key, source_kind = excluded.source_kind";

/// Delete every scanned row, not a grouping's, stamped with another
/// generation than `generation`, ending each candidate's identification, and
/// return their keys.
pub(super) fn prune_other_generations(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
) -> Result<Vec<CandidateKey>, DbError> {
    let pruned: Vec<CandidateKey> = sql.query(
        "SELECT path FROM scan_candidate \
         WHERE watched_folder_path = ? AND generation != ? AND source_kind = 'folder' \
         ORDER BY path",
        params![coven::path_bytes(watched_folder_path), generation],
        |row| row.get::<_, CandidateKey>(0),
    )?;
    sql.execute(
        "DELETE FROM scan_candidate WHERE watched_folder_path = ? AND generation != ? AND source_kind = 'folder'",
        params![coven::path_bytes(watched_folder_path), generation],
    )?;
    // The candidates the pass did not find leave the scan, and their
    // identifications with them.
    for key in &pruned {
        crate::tables::import_state::end_file_work_unless_of_on(sql, key, None)?;
    }
    Ok(pruned)
}

/// Stamp a candidate row and its file-tag reading with `generation`, changing
/// nothing else, so the completion prune keeps the row. The row keeps the
/// files the reading was taken from, so the reading stays current with it.
pub(crate) fn touch_candidate(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    path: &CandidateKey,
    generation: i64,
) -> Result<(), DbError> {
    sql.execute(
        "UPDATE scan_candidate SET generation = ? WHERE watched_folder_path = ? AND path = ?",
        params![generation, coven::path_bytes(watched_folder_path), path],
    )?;
    sql.execute(
        "UPDATE scan_candidate_tag_snapshot SET scan_generation = ? \
         WHERE watched_folder_path = ? AND candidate_path = ?",
        params![generation, coven::path_bytes(watched_folder_path), path],
    )?;
    Ok(())
}

pub(super) fn insert_item(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    item: &ScanItem,
    file_metadata: Option<&crate::file_metadata_seed::FileMetadataSeed>,
    source: super::EntrySource,
) -> Result<(), DbError> {
    let source_kind = match source {
        super::EntrySource::Scanned { .. } => "folder",
        super::EntrySource::Grouping => "grouping",
    };
    match item {
        ScanItem::Discovered(candidate) => insert_candidate(
            sql,
            watched_folder_path,
            generation,
            "tentative",
            source_kind,
            candidate,
            file_metadata,
        ),
        ScanItem::Valid(candidate) => insert_candidate(
            sql,
            watched_folder_path,
            generation,
            "valid",
            source_kind,
            candidate,
            file_metadata,
        ),
        ScanItem::Invalid(candidate) => {
            insert_invalid(sql, watched_folder_path, generation, source_kind, candidate)
        }
    }
}

fn insert_candidate(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    kind: &str,
    source_kind: &str,
    candidate: &FolderCandidate,
    file_metadata: Option<&crate::file_metadata_seed::FileMetadataSeed>,
) -> Result<(), DbError> {
    insert_candidate_row(sql, watched_folder_path, generation, kind, source_kind, candidate)?;
    let path = candidate.key();
    use crate::tables::import_state::ensure_candidate_state;
    // File rows first: the file-tag reading references them, and the blank
    // draft is read from the units stored with them.
    insert_candidate_files(sql, watched_folder_path, &path, &candidate.files)?;
    ensure_candidate_state(sql, &path, &candidate.path, &candidate.files, file_metadata)?;
    // The reading belongs to the scan row, so a seed always stores it, while
    // its draft only fills a candidate that has none.
    if let Some(seed) = file_metadata {
        super::tag_snapshot::replace_candidate_file_tag_snapshot(
            sql,
            watched_folder_path,
            &path,
            &seed.snapshot,
        )?;
    }
    Ok(())
}

/// The statement writing a candidate's `scan_candidate` row over the one
/// stored under its key.
fn candidate_row_insert() -> String {
    format!(
        "INSERT INTO scan_candidate \
             (watched_folder_path, path, generation, kind, name, relative_folder, folder, \
              file_root, scope, content_hash, file_edit_revision, grouping_key, \
              source_kind) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) {OVER_STORED_ROW}"
    )
}

/// Write one candidate's `scan_candidate` row, over the one stored under its
/// key, and its parts. `source_kind` is `folder` for a scan's row, `grouping`
/// for a grouping's.
pub(crate) fn insert_candidate_row(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    kind: &str,
    source_kind: &str,
    candidate: &FolderCandidate,
) -> Result<(), DbError> {
    let path = candidate.key();
    sql.execute(
        &candidate_row_insert(),
        params![
            coven::path_bytes(watched_folder_path),
            path,
            generation,
            kind,
            candidate.name,
            candidate.relative_folder,
            coven::path_bytes(&candidate.path),
            coven::path_bytes(&candidate.file_root),
            scope_text(candidate.scope),
            candidate.files.content_hash(),
            to_i64(candidate.file_edit_revision, "a candidate's file edit revision")?,
            candidate.grouping,
            source_kind,
        ],
    )?;
    for (position, part) in candidate.files.parts.iter().enumerate() {
        sql.execute(
            "INSERT INTO scan_candidate_part \
                 (watched_folder_path, candidate_path, position, folder, prefix) \
             VALUES (?, ?, ?, ?, ?)",
            params![
                coven::path_bytes(watched_folder_path),
                path,
                to_i64(position as u64, "a release part's position")?,
                coven::path_bytes(&part.folder),
                part.prefix,
            ],
        )?;
    }
    Ok(())
}

fn insert_invalid(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    source_kind: &str,
    candidate: &InvalidCandidate,
) -> Result<(), DbError> {
    let folder = coven::path_bytes(&candidate.path);
    let key = candidate.key();
    sql.execute(
        &format!(
            "INSERT INTO scan_candidate \
                 (watched_folder_path, path, generation, kind, name, relative_folder, folder, \
                  file_root, scope, content_hash, file_edit_revision, grouping_key, \
                  source_kind) \
             VALUES (?, ?, ?, 'invalid', ?, ?, ?, ?, ?, NULL, 0, ?, ?) {OVER_STORED_ROW}"
        ),
        params![
            coven::path_bytes(watched_folder_path),
            key,
            generation,
            candidate.name,
            candidate.relative_folder,
            folder,
            folder,
            scope_text(if candidate.grouping.is_some() {
                crate::folder_scanner::ReleaseFileScope::Recursive
            } else {
                crate::folder_scanner::ReleaseFileScope::Direct
            }),
            candidate.grouping,
            source_kind,
        ],
    )?;
    for (position, file) in candidate.files.iter().enumerate() {
        let columns = proposed_role_columns(file.role);
        sql.execute(
            "INSERT INTO scan_skipped_file \
                 (watched_folder_path, candidate_path, position, relative_path, absolute_path, \
                  size, modified_at_ns, role, audio_damage, document_kind) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                coven::path_bytes(watched_folder_path),
                key,
                to_i64(position as u64, "a skipped file's position")?,
                file.relative_path,
                coven::path_bytes(&file.file.path),
                to_i64(file.file.size, "a skipped file's size")?,
                file.file.modified_at_ns,
                columns.role,
                columns.audio_damage,
                columns.document_kind,
            ],
        )?;
    }
    if let Some(tags) = &candidate.tags {
        sql.execute(
            "INSERT INTO scan_skipped_tags \
                 (watched_folder_path, candidate_path, album_title, credits) \
             VALUES (?, ?, ?, ?)",
            params![
                coven::path_bytes(watched_folder_path),
                key,
                tags.album_title,
                tags.artist.as_ref().map(crate::tables::stored_credits::line_json).transpose()?,
            ],
        )?;
    }
    Ok(())
}

/// Insert one candidate's files and their parsed track sheets; a file decision
/// uses it too, to replace the files the scan proposed. Each sheet's audio
/// links go in last, since they reference file rows that may sort after the
/// sheet's.
pub(crate) fn insert_candidate_files(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    files: &crate::folder_scanner::CategorizedFiles,
) -> Result<(), DbError> {
    for (position, file) in files.files.iter().enumerate() {
        insert_file(sql, watched_folder_path, candidate_path, position, file)?;
    }
    for sheet in files.track_sheets() {
        let Some(audio_files) = sheet.binding.audio_files() else {
            continue;
        };
        for (position, audio) in audio_files.iter().enumerate() {
            sql.execute(
                "INSERT INTO scan_sheet_audio_file \
                     (watched_folder_path, candidate_path, sheet_relative_path, position, \
                      file_reference, audio_relative_path) \
                 VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    coven::path_bytes(watched_folder_path),
                    candidate_path,
                    sheet.file.relative_path,
                    to_i64(position as u64, "a sheet audio file's position")?,
                    audio.file_reference,
                    audio.file_id,
                ],
            )?;
        }
    }
    sql.execute(
        "UPDATE scan_candidate SET blank_cover_file = ? \
         WHERE watched_folder_path = ? AND path = ?",
        params![
            crate::cover_art::blank_cover_file(files.artwork()),
            coven::path_bytes(watched_folder_path),
            candidate_path
        ],
    )?;
    super::units::insert_units(sql, watched_folder_path, candidate_path, files)
}

fn insert_file(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    position: usize,
    file: &CandidateFile,
) -> Result<(), DbError> {
    let columns = role_columns(&file.role);
    sql.execute(
        "INSERT INTO scan_candidate_file \
             (watched_folder_path, candidate_path, relative_path, position, absolute_path, \
              size, modified_at_ns, audio_content_type, audio_container, audio_duration_ms, \
              audio_sample_rate_hz, audio_bits_per_sample, audio_bitrate_kbps, audio_channels, \
              file_name, dir_prefix, role, document_kind, sheet_binding, \
              sheet_binding_codec, sheet_disc, sheet_disc_number) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            coven::path_bytes(watched_folder_path),
            candidate_path,
            file.file.relative_path,
            to_i64(position as u64, "a candidate file's position")?,
            coven::path_bytes(&file.file.version.path),
            to_i64(file.file.version.size, "a file's size")?,
            file.file.version.modified_at_ns,
            file.file.source_audio.as_ref().map(|audio| audio.content_type.as_str()),
            file.file.source_audio.as_ref().map(|audio| audio.container.as_str()),
            file.file
                .source_audio
                .as_ref()
                .map(|audio| to_i64(audio.duration_ms, "an audio file's duration"))
                .transpose()?,
            file.file.source_audio.as_ref().map(|audio| audio.format.sample_rate_hz),
            file.file.source_audio.as_ref().and_then(|audio| audio.format.bits_per_sample),
            file.file.source_audio.as_ref().and_then(|audio| audio.format.bitrate_kbps),
            file.file.source_audio.as_ref().map(|audio| audio.format.channels),
            file.file.file_name,
            file.file.dir_prefix,
            columns.role,
            columns.document_kind,
            columns.sheet_binding,
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
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    sheet_relative_path: &str,
    sheet: &CueSheet,
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO scan_cue_sheet \
             (watched_folder_path, candidate_path, sheet_relative_path, title, performer, \
              catalog, date, ripper) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            coven::path_bytes(watched_folder_path),
            candidate_path,
            sheet_relative_path,
            sheet.title,
            sheet.performer,
            sheet.catalog,
            sheet.date,
            sheet.ripper.map(ripper_text),
        ],
    )?;
    for (position, track) in sheet.tracks.iter().enumerate() {
        let position = to_i64(position as u64, "a cue track's position")?;
        let (mode, mode_other) = match &track.mode {
            crate::cue_sheet::CueTrackMode::Audio => ("audio", None),
            crate::cue_sheet::CueTrackMode::Other(other) => ("other", Some(other.as_str())),
        };
        sql.execute(
            "INSERT INTO scan_cue_track \
                 (watched_folder_path, candidate_path, sheet_relative_path, position, number, \
                  mode, mode_other, title, performer, file_reference, start_cue_frames, \
                  end_cue_frames, generated_pregap_frames) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                coven::path_bytes(watched_folder_path),
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
                track
                    .generated_pregap_frames
                    .map(|frames| to_i64(frames, "a generated pregap's length"))
                    .transpose()?,
            ],
        )?;
        for (index_position, index) in track.indexes.iter().enumerate() {
            sql.execute(
                "INSERT INTO scan_cue_index \
                     (watched_folder_path, candidate_path, sheet_relative_path, track_position, \
                      position, number, frames, file_reference) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    coven::path_bytes(watched_folder_path),
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

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
