//! Writing one scan item as rows. The caller deletes the item's
//! `scan_candidate` row first, and every table below it goes with it by
//! cascade, so these inserts always write into empty space.

use super::columns::*;
use super::*;
use crate::cue_flac::{CuePregap, CueSheet};
use crate::import::folder_scanner::{
    CandidateFile, Coverage, FileRole, FolderCandidate, InvalidCandidate, ScanItem,
};

/// A stored entry's key and the files it reads, which is all a superseding
/// write needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredEntry {
    pub(crate) key: String,
    pub(crate) coverage: Coverage,
}

pub(crate) fn delete_entry(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    key: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM scan_candidate WHERE watched_folder_path = ? AND path = ?",
        params![watched_folder_path, key],
    )?;
    Ok(())
}

/// Replace one candidate's stored file-tag reading, inside the caller's
/// transaction.
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

/// Delete every scanned row, not a grouping's, stamped with another
/// generation than `generation`, and return their keys.
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

/// Stamp a candidate row and its file-tag reading with `generation`, changing
/// nothing else, so the completion prune keeps the row. The row keeps the
/// files the reading was taken from, so the reading stays current with it.
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
    sql.execute(
        "UPDATE scan_candidate_tag_snapshot SET scan_generation = ? \
         WHERE watched_folder_path = ? AND candidate_path = ?",
        params![generation, watched_folder_path, path],
    )?;
    Ok(())
}

pub(super) fn insert_item(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    item: &ScanItem,
    file_metadata: Option<&crate::import::file_metadata_seed::FileMetadataSeed>,
    source: super::EntrySource,
) -> Result<(), DbError> {
    let source_kind = match source {
        super::EntrySource::Scanned => "folder",
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
        ScanItem::Decided { .. } => Err(DbError::Message(
            "a folder reading is stored as a decision, not as a scan entry".to_string(),
        )),
        ScanItem::Sidecar(_) => Err(DbError::Message(
            "a folder's sidecar files are stored as a sidecar, not as a scan entry".to_string(),
        )),
    }
}

fn insert_candidate(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    kind: &str,
    source_kind: &str,
    candidate: &FolderCandidate,
    file_metadata: Option<&crate::import::file_metadata_seed::FileMetadataSeed>,
) -> Result<(), DbError> {
    insert_candidate_row(
        sql,
        watched_folder_path,
        generation,
        kind,
        source_kind,
        candidate,
    )?;
    let path = candidate.key();
    let blank;
    let seed = match file_metadata {
        Some(seed) => CandidateStateSeed::FileMetadata(seed),
        None => {
            blank = candidate.blank_source();
            CandidateStateSeed::Blank(&blank)
        }
    };
    // File rows first: the file-tag reading references them.
    insert_candidate_files(sql, watched_folder_path, &path, &candidate.files)?;
    ensure_candidate_state(sql, &path, &candidate.path, &candidate.files, seed)?;
    // The reading belongs to the scan row, so a seed always stores it, while
    // its draft only fills a candidate that has none.
    if let Some(seed) = file_metadata {
        replace_candidate_file_tag_snapshot(sql, watched_folder_path, &path, &seed.snapshot)?;
    }
    Ok(())
}

/// Insert one candidate's `scan_candidate` row and its parts. `source_kind` is
/// `folder` for a scan's row, `grouping` for a grouping's.
pub(crate) fn insert_candidate_row(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    kind: &str,
    source_kind: &str,
    candidate: &FolderCandidate,
) -> Result<(), DbError> {
    let path = candidate.key();
    sql.execute(
        "INSERT INTO scan_candidate \
             (watched_folder_path, path, generation, kind, name, display_path, folder, \
              file_root, scope, content_hash, file_edit_revision, grouping_key, source_kind, \
              invalid_reason, invalid_reason_path) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)",
        params![
            watched_folder_path,
            path,
            generation,
            kind,
            candidate.name,
            candidate.display_path,
            candidate.path.to_string_lossy(),
            candidate.file_root.to_string_lossy(),
            scope_text(candidate.scope),
            candidate.files.content_hash(),
            to_i64(
                candidate.file_edit_revision,
                "a candidate's file edit revision"
            )?,
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
                watched_folder_path,
                path,
                to_i64(position as u64, "a release part's position")?,
                part.folder.to_string_lossy(),
                part.prefix,
            ],
        )?;
    }
    Ok(())
}

/// What a candidate's draft is created from when it has none.
pub(crate) enum CandidateStateSeed<'a> {
    /// As many empty track slots as the folder has audio units.
    Blank(&'a crate::import::pane::CandidateSourceDraft),
    /// The folder read as its own files describe it, with the reading it was
    /// projected from and the cover those tags embed.
    FileMetadata(&'a crate::import::file_metadata_seed::FileMetadataSeed),
}

/// Make sure `files` has a state row, seeding its draft and cover when it has
/// none, and record `folder` as a folder on disk it was found at; the state
/// lives while a watched folder covers one of those folders.
pub(crate) fn ensure_candidate_state(
    sql: &SqlContext<'_, '_>,
    path: &str,
    folder: &std::path::Path,
    files: &crate::import::folder_scanner::CategorizedFiles,
    seed: CandidateStateSeed<'_>,
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
        "INSERT INTO import_candidate_folder (content_hash, folder) \
         VALUES (?, ?) ON CONFLICT DO NOTHING",
        params![content_hash, folder.to_string_lossy()],
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
        match seed {
            CandidateStateSeed::Blank(source) => {
                super::super::import_state::insert_draft(
                    sql,
                    &content_hash,
                    &source.draft,
                    crate::import::MetadataAuthor::Nobody,
                )?;
            }
            CandidateStateSeed::FileMetadata(seed) => {
                super::super::import_state::insert_file_tags_draft(
                    sql,
                    &content_hash,
                    &seed.draft,
                )?;
            }
        }
    }
    // The folder's own cover is stored with the candidate so readers never
    // recompute it; a candidate that already has a cover keeps it.
    let has_cover = sql
        .query_row(
            "SELECT 1 FROM import_candidate_cover WHERE content_hash = ?",
            [&content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_cover {
        let embedded = match seed {
            CandidateStateSeed::Blank(_) => None,
            CandidateStateSeed::FileMetadata(seed) => seed.cover.clone(),
        };
        if let Some(cover) = crate::import::local_artwork::folder_cover(embedded, files.artwork()) {
            super::super::candidate_state_rows::save_cover(sql, &content_hash, &cover)?;
        }
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
    source_kind: &str,
    candidate: &InvalidCandidate,
) -> Result<(), DbError> {
    let (reason, reason_path) = invalid_reason_columns(&candidate.reason);
    let folder = candidate.path.to_string_lossy();
    sql.execute(
        "INSERT INTO scan_candidate \
             (watched_folder_path, path, generation, kind, name, display_path, folder, \
              file_root, scope, content_hash, file_edit_revision, grouping_key, \
              source_kind, invalid_reason, invalid_reason_path) \
         VALUES (?, ?, ?, 'invalid', ?, ?, ?, ?, ?, NULL, 0, ?, ?, ?, ?)",
        params![
            watched_folder_path,
            candidate.key(),
            generation,
            candidate.name,
            candidate.display_path,
            folder,
            folder,
            scope_text(if candidate.grouping.is_some() {
                crate::import::folder_scanner::ReleaseFileScope::Recursive
            } else {
                crate::import::folder_scanner::ReleaseFileScope::Direct
            }),
            candidate.grouping,
            source_kind,
            reason,
            reason_path,
        ],
    )?;
    Ok(())
}

/// Insert one candidate's files and their parsed track sheets; a file decision
/// uses it too, to replace the files the scan proposed. Each sheet's audio
/// links go in last, since they reference file rows that may sort after the
/// sheet's.
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
              catalog, date, ripper) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            watched_folder_path,
            candidate_path,
            sheet_relative_path,
            sheet.title,
            sheet.performer,
            sheet.catalog,
            sheet.date,
            sheet.ripper.map(crate::cue_flac::CdRipper::key),
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
