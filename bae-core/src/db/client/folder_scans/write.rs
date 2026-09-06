//! Writing one scan item down as rows. A saved item replaces what was stored
//! under its key: the candidate or boundary row is deleted first, and every
//! table below it goes with it by cascade, so the insert that follows is
//! always writing into empty space.

use super::columns::*;
use super::*;
use crate::cue_flac::{CuePregap, CueSheet};
use crate::import::folder_scanner::{
    CandidateFile, FileRole, FolderCandidate, InvalidCandidate, ResolvedFolderReleaseBoundary,
    ScanItem,
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
    }
    Ok(())
}

/// Replace one candidate's stored file-tag reading. The caller has already
/// compared the snapshot stamp with the current candidate inside this write
/// transaction, so deleting first cannot expose a partial replacement.
pub(crate) fn replace_candidate_file_tag_snapshot(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM scan_candidate_tag_snapshot \
         WHERE watched_folder_path = ? AND candidate_path = ?",
        params![watched_folder_path, candidate_path],
    )?;
    let (cover_source, cover_content_type, cover_data) = match &snapshot.embedded_cover {
        Some(cover) => (
            Some(cover.source_relative_path.as_str()),
            Some(cover.content_type.as_str()),
            Some(cover.data.as_slice()),
        ),
        None => (None, None, None),
    };
    sql.execute(
        "INSERT INTO scan_candidate_tag_snapshot \
             (watched_folder_path, candidate_path, scan_generation, file_edit_revision, \
              embedded_cover_source_relative_path, embedded_cover_content_type, \
              embedded_cover_data) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            watched_folder_path,
            candidate_path,
            to_i64(
                snapshot.scan_generation,
                "a file-tag snapshot's scan generation"
            )?,
            to_i64(
                snapshot.file_edit_revision,
                "a file-tag snapshot's file edit revision"
            )?,
            cover_source,
            cover_content_type,
            cover_data,
        ],
    )?;
    for fact in &snapshot.files {
        sql.execute(
            "INSERT INTO scan_candidate_file_tag \
                 (watched_folder_path, candidate_path, relative_path, file_size, \
                  modified_at_ns, title, track_artist, album_title, \
                  album_artist, year, track_number, disc_number) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                watched_folder_path,
                candidate_path,
                fact.observation.relative_path,
                to_i64(fact.observation.size, "a file-tag observation's size")?,
                fact.observation.modified_at_ns,
                fact.title,
                fact.track_artist,
                fact.album_title,
                fact.album_artist,
                fact.year.map(i64::from),
                fact.track_number.map(i64::from),
                fact.disc_number.map(i64::from),
            ],
        )?;
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
         WHERE watched_folder_path = ? AND generation != ? AND source_kind = 'folder'",
        params![watched_folder_path, generation],
        |row| row.get::<_, String>(0),
    )?;
    sql.execute(
        "DELETE FROM scan_candidate WHERE watched_folder_path = ? AND generation != ? AND source_kind = 'folder'",
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

pub(super) fn insert_item(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    item: &ScanItem,
    initial_metadata_source: crate::config::DefaultImportMetadataSource,
) -> Result<(), DbError> {
    match item {
        ScanItem::Discovered(candidate) => insert_candidate(
            sql,
            watched_folder_path,
            generation,
            "tentative",
            candidate,
            initial_metadata_source,
        ),
        ScanItem::Valid(candidate) => insert_candidate(
            sql,
            watched_folder_path,
            generation,
            "valid",
            candidate,
            initial_metadata_source,
        ),
        ScanItem::Invalid(candidate) => {
            insert_invalid(sql, watched_folder_path, generation, candidate)
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
    initial_metadata_source: crate::config::DefaultImportMetadataSource,
) -> Result<(), DbError> {
    let path = candidate.path.to_string_lossy().into_owned();
    sql.execute(
        "INSERT INTO scan_candidate \
             (watched_folder_path, path, generation, kind, name, display_path, file_root, \
              scope, content_hash, file_edit_revision, initial_metadata_source, \
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
            initial_metadata_source.as_str(),
            candidate
                .combine_ancestor_key
                .as_ref()
                .map(|key| key.relative_folder_path.as_str()),
        ],
    )?;
    ensure_candidate_state(
        sql,
        &path,
        watched_folder_path,
        &candidate.files,
        &crate::import::pane::blank_candidate_source(&candidate.files),
    )?;
    insert_candidate_files(sql, watched_folder_path, &path, &candidate.files)?;
    insert_resolved_boundaries(
        sql,
        watched_folder_path,
        &path,
        &candidate.resolved_boundaries,
    )
}

pub(crate) fn ensure_candidate_state(
    sql: &SqlContext<'_, '_>,
    path: &str,
    watched_folder_path: &str,
    files: &crate::import::folder_scanner::CategorizedFiles,
    source_draft: &crate::import::pane::CandidateSourceDraft,
) -> Result<(), DbError> {
    let content_hash = files.content_hash();
    let created = sql.execute(
        "INSERT INTO import_candidate_state (content_hash, folder_path) VALUES (?, ?) \
         ON CONFLICT (content_hash) DO NOTHING",
        params![content_hash, path],
    )? == 1;
    if !created {
        sql.execute(
            "UPDATE import_candidate_state SET folder_path = ? WHERE content_hash = ?",
            params![path, content_hash],
        )?;
    }
    sql.execute(
        "INSERT INTO import_candidate_watched_root (content_hash, watched_folder_path) \
         VALUES (?, ?) ON CONFLICT DO NOTHING",
        params![content_hash, watched_folder_path],
    )?;
    let has_draft = sql
        .query_row(
            "SELECT 1 FROM import_candidate_edit WHERE content_hash = ?",
            [&content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_draft {
        super::super::import_state::insert_draft(sql, &content_hash, &source_draft.draft)?;
    }
    if created {
        sql.execute(
            "INSERT INTO import_candidate_asset_preparation (content_hash) VALUES (?)",
            [&content_hash],
        )?;
    }
    Ok(())
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
              scope, content_hash, file_edit_revision, initial_metadata_source, \
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
///
/// The audio each bound sheet describes goes in last: those rows reference
/// the audio's own file row, which may sort after the sheet's.
pub(crate) fn insert_candidate_files(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    files: &crate::import::folder_scanner::CategorizedFiles,
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
                    watched_folder_path,
                    candidate_path,
                    sheet.file.relative_path,
                    to_i64(position as u64, "a sheet audio file's position")?,
                    audio.file_reference,
                    audio.file_id,
                ],
            )?;
        }
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
              size, modified_at_ns, audio_content_type, audio_duration_ms, \
              audio_sample_rate_hz, audio_bits_per_sample, audio_bitrate_kbps, audio_channels, \
              file_name, dir_prefix, proposed_audio, role, sheet_binding, \
              sheet_binding_codec, sheet_disc, sheet_disc_number) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            watched_folder_path,
            candidate_path,
            file.file.relative_path,
            to_i64(position as u64, "a candidate file's position")?,
            file.file.path.to_string_lossy(),
            to_i64(file.file.size, "a file's size")?,
            file.file.modified_at_ns,
            file.file
                .source_audio
                .as_ref()
                .map(|audio| audio.content_type.as_str()),
            file.file
                .source_audio
                .as_ref()
                .map(|audio| to_i64(audio.duration_ms, "an audio file's duration"))
                .transpose()?,
            file.file
                .source_audio
                .as_ref()
                .map(|audio| audio.format.sample_rate_hz),
            file.file
                .source_audio
                .as_ref()
                .and_then(|audio| audio.format.bits_per_sample),
            file.file
                .source_audio
                .as_ref()
                .and_then(|audio| audio.format.bitrate_kbps),
            file.file
                .source_audio
                .as_ref()
                .map(|audio| audio.format.channels),
            file.file.file_name,
            file.file.dir_prefix,
            file.proposed_audio,
            columns.role,
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
