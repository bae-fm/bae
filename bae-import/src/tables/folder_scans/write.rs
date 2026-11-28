//! Writing one scan item as rows. An item written again keeps its
//! `scan_candidate` row, which the write updates in place, and clears every
//! table below it first ([`clear_entry_rows`]), so these inserts always write
//! into empty space while what the import list holds against the row (its
//! place, whether it is selected, its group's fold) stays with it.

use super::columns::*;
use super::*;
use crate::cue_sheet::CueSheet;
use crate::folder_scanner::{
    CandidateFile, FileRole, FolderCandidate, InvalidCandidate, InvalidReason, ScanItem,
};
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

coven::write!(
    DeleteScanCandidate(watched_folder_path: &'p [u8], key: &'p crate::CandidateKey),
    r#"
    DELETE FROM scan_candidate WHERE watched_folder_path = :watched_folder_path AND path = :key
"#
);

fn delete_entry_row(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    key: &CandidateKey,
) -> Result<(), DbError> {
    sql.run(&DeleteScanCandidateParams {
        watched_folder_path: &coven::path_bytes(watched_folder_path),
        key,
    })?;
    Ok(())
}

coven::write!(
    DeleteEntryTagSnapshot(watched_folder_path: &'p [u8], key: &'p crate::CandidateKey),
    r#"
    DELETE FROM scan_candidate_tag_snapshot
    WHERE watched_folder_path = :watched_folder_path AND candidate_path = :key
"#
);

coven::write!(
    DeleteEntryUnits(watched_folder_path: &'p [u8], key: &'p crate::CandidateKey),
    r#"
    DELETE FROM scan_candidate_unit
    WHERE watched_folder_path = :watched_folder_path AND candidate_path = :key
"#
);

coven::write!(
    DeleteEntryParts(watched_folder_path: &'p [u8], key: &'p crate::CandidateKey),
    r#"
    DELETE FROM scan_candidate_part
    WHERE watched_folder_path = :watched_folder_path AND candidate_path = :key
"#
);

coven::write!(
    DeleteEntryFiles(watched_folder_path: &'p [u8], key: &'p crate::CandidateKey),
    r#"
    DELETE FROM scan_candidate_file
    WHERE watched_folder_path = :watched_folder_path AND candidate_path = :key
"#
);

coven::write!(
    DeleteEntrySkippedFiles(watched_folder_path: &'p [u8], key: &'p crate::CandidateKey),
    r#"
    DELETE FROM scan_skipped_file
    WHERE watched_folder_path = :watched_folder_path AND candidate_path = :key
"#
);

coven::write!(
    DeleteEntrySkippedTags(watched_folder_path: &'p [u8], key: &'p crate::CandidateKey),
    r#"
    DELETE FROM scan_skipped_tags
    WHERE watched_folder_path = :watched_folder_path AND candidate_path = :key
"#
);

/// Declare [`clear_entry_rows`] over one delete for each table that hangs off
/// a `scan_candidate` row by foreign key, and list the deletes' SQL for the
/// test that holds them against the schema's foreign keys.
macro_rules! entry_row_clears {
    ($($write:ident => $params:ident),+ $(,)?) => {
        #[cfg(test)]
        const ENTRY_ROW_CLEARS: &[&str] = &[$($write::SQL),+];

        /// Clear every row below the entry's `scan_candidate` row, for a write
        /// that stores the entry again over that row and says itself what
        /// that does to its candidate's identification.
        pub(super) fn clear_entry_rows(
            sql: &CovenWrite<'_>,
            watched_folder_path: &Path,
            key: &CandidateKey,
        ) -> Result<(), DbError> {
            let watched_folder_path = coven::path_bytes(watched_folder_path);
            $(sql.run(&$params { watched_folder_path: &watched_folder_path, key })?;)+
            Ok(())
        }
    };
}

entry_row_clears!(
    DeleteEntryTagSnapshot => DeleteEntryTagSnapshotParams,
    DeleteEntryUnits => DeleteEntryUnitsParams,
    DeleteEntryParts => DeleteEntryPartsParams,
    DeleteEntryFiles => DeleteEntryFilesParams,
    DeleteEntrySkippedFiles => DeleteEntrySkippedFilesParams,
    DeleteEntrySkippedTags => DeleteEntrySkippedTagsParams,
);

coven::write!(
    DeleteOtherGenerations(watched_folder_path: &'p [u8], generation: i64),
    r#"
    DELETE FROM scan_candidate
    WHERE watched_folder_path = :watched_folder_path AND generation != :generation
        AND source_kind = 'folder'
"#
);

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
    sql.run(&DeleteOtherGenerationsParams {
        watched_folder_path: &coven::path_bytes(watched_folder_path),
        generation,
    })?;
    // The candidates the pass did not find leave the scan, and their
    // identifications with them.
    for key in &pruned {
        crate::tables::import_state::end_file_work_unless_of_on(sql, key, None)?;
    }
    Ok(pruned)
}

coven::write!(
    StampScanCandidate(
        watched_folder_path: &'p [u8],
        path: &'p crate::CandidateKey,
        generation: i64,
    ),
    r#"
    UPDATE scan_candidate SET generation = :generation
    WHERE watched_folder_path = :watched_folder_path AND path = :path
"#
);

coven::write!(
    StampTagSnapshot(watched_folder_path: &'p [u8], path: &'p crate::CandidateKey, generation: i64),
    r#"
    UPDATE scan_candidate_tag_snapshot SET scan_generation = :generation
    WHERE watched_folder_path = :watched_folder_path AND candidate_path = :path
"#
);

/// Stamp a candidate row and its file-tag reading with `generation`, changing
/// nothing else, so the completion prune keeps the row. The row keeps the
/// files the reading was taken from, so the reading stays current with it.
pub(crate) fn touch_candidate(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    path: &CandidateKey,
    generation: i64,
) -> Result<(), DbError> {
    let watched_folder_path = coven::path_bytes(watched_folder_path);
    sql.run(&StampScanCandidateParams {
        watched_folder_path: &watched_folder_path,
        path,
        generation,
    })?;
    sql.run(&StampTagSnapshotParams {
        watched_folder_path: &watched_folder_path,
        path,
        generation,
    })?;
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
    // Insert the file rows first, because the file-tag reading references
    // them and `ensure_candidate_state` builds the blank draft from the units
    // stored with them.
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

// A candidate's row, valid or invalid, written over the one stored under its
// key, column for column. Only an invalid candidate has the `invalid_*`
// columns, and it has no content hash.
coven::write!(
    UpsertCandidateRow(
        watched_folder_path: &'p [u8],
        path: &'p crate::CandidateKey,
        generation: i64,
        kind: &'p str,
        name: &'p str,
        relative_folder: &'p crate::RelativeFolder,
        folder: Vec<u8>,
        file_root: Vec<u8>,
        scope: &'static str,
        content_hash: Option<String>,
        file_edit_revision: i64,
        grouping_key: Option<&'p crate::CandidateKey>,
        source_kind: &'p str,
        blank_cover_file: Option<String>,
        invalid_reason: Option<&'static str>,
        invalid_file_name: Option<String>,
        invalid_damaged_audio: Option<u32>,
        invalid_audio_bits: Option<u32>,
    ),
    r#"
    INSERT INTO scan_candidate
        (watched_folder_path, path, generation, kind, name, relative_folder, folder, file_root,
         scope, content_hash, file_edit_revision, grouping_key, source_kind, blank_cover_file,
         invalid_reason, invalid_file_name, invalid_damaged_audio, invalid_audio_bits)
    VALUES (:watched_folder_path, :path, :generation, :kind, :name, :relative_folder, :folder,
        :file_root, :scope, :content_hash, :file_edit_revision, :grouping_key, :source_kind,
        :blank_cover_file, :invalid_reason, :invalid_file_name, :invalid_damaged_audio,
        :invalid_audio_bits)
    ON CONFLICT (watched_folder_path, path) DO UPDATE SET
        generation = excluded.generation, kind = excluded.kind, name = excluded.name,
        relative_folder = excluded.relative_folder, folder = excluded.folder,
        file_root = excluded.file_root, scope = excluded.scope,
        content_hash = excluded.content_hash,
        file_edit_revision = excluded.file_edit_revision,
        grouping_key = excluded.grouping_key, source_kind = excluded.source_kind,
        blank_cover_file = excluded.blank_cover_file,
        invalid_reason = excluded.invalid_reason,
        invalid_file_name = excluded.invalid_file_name,
        invalid_damaged_audio = excluded.invalid_damaged_audio,
        invalid_audio_bits = excluded.invalid_audio_bits
"#
);

coven::write!(
    InsertCandidatePart(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        position: i64,
        folder: Vec<u8>,
        prefix: &'p str,
    ),
    r#"
    INSERT INTO scan_candidate_part
        (watched_folder_path, candidate_path, position, folder, prefix)
    VALUES (:watched_folder_path, :candidate_path, :position, :folder, :prefix)
"#
);

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
    let watched_folder_path = coven::path_bytes(watched_folder_path);
    sql.run(&UpsertCandidateRowParams {
        watched_folder_path: &watched_folder_path,
        path: &path,
        generation,
        kind,
        name: &candidate.name,
        relative_folder: &candidate.relative_folder,
        folder: coven::path_bytes(&candidate.path),
        file_root: coven::path_bytes(&candidate.file_root),
        scope: scope_text(candidate.scope),
        content_hash: Some(candidate.files.content_hash()),
        file_edit_revision: to_i64(
            candidate.file_edit_revision,
            "a candidate's file edit revision",
        )?,
        grouping_key: candidate.grouping.as_ref(),
        source_kind,
        // `insert_candidate_files` sets it from the files it stores.
        blank_cover_file: None,
        invalid_reason: None,
        invalid_file_name: None,
        invalid_damaged_audio: None,
        invalid_audio_bits: None,
    })?;
    for (position, part) in candidate.files.parts.iter().enumerate() {
        sql.run(&InsertCandidatePartParams {
            watched_folder_path: &watched_folder_path,
            candidate_path: &path,
            position: to_i64(position as u64, "a release part's position")?,
            folder: coven::path_bytes(&part.folder),
            prefix: &part.prefix,
        })?;
    }
    Ok(())
}

coven::write!(
    InsertSkippedFile(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        position: i64,
        relative_path: &'p str,
        absolute_path: Vec<u8>,
        size: i64,
        modified_at_ns: i64,
        role: &'static str,
        audio_damage: Option<&'static str>,
        audio_bits: Option<u32>,
        document_kind: Option<&'static str>,
    ),
    r#"
    INSERT INTO scan_skipped_file
        (watched_folder_path, candidate_path, position, relative_path, absolute_path, size,
         modified_at_ns, role, audio_damage, audio_bits, document_kind)
    VALUES (:watched_folder_path, :candidate_path, :position, :relative_path, :absolute_path,
        :size, :modified_at_ns, :role, :audio_damage, :audio_bits, :document_kind)
"#
);

coven::write!(
    InsertSkippedTags(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        album_title: &'p str,
        credits: Option<String>,
    ),
    r#"
    INSERT INTO scan_skipped_tags (watched_folder_path, candidate_path, album_title, credits)
    VALUES (:watched_folder_path, :candidate_path, :album_title, :credits)
"#
);

fn insert_invalid(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    source_kind: &str,
    candidate: &InvalidCandidate,
) -> Result<(), DbError> {
    let folder = coven::path_bytes(&candidate.path);
    let key = candidate.key();
    let (reason, file_name, damaged_audio, audio_bits) = match candidate.reason() {
        InvalidReason::SameName { name } => ("same_name", Some(name), None, None),
        InvalidReason::DamagedAudio { count } => ("damaged_audio", None, Some(count), None),
        InvalidReason::DeepAudio { name, bits } => ("deep_audio", Some(name), None, Some(bits)),
        InvalidReason::NoValidAudio => ("no_valid_audio", None, None, None),
    };
    let watched_folder_path = coven::path_bytes(watched_folder_path);
    sql.run(&UpsertCandidateRowParams {
        watched_folder_path: &watched_folder_path,
        path: &key,
        generation,
        kind: "invalid",
        name: &candidate.name,
        relative_folder: &candidate.relative_folder,
        folder: folder.clone(),
        file_root: folder,
        scope: scope_text(if candidate.grouping.is_some() {
            crate::folder_scanner::ReleaseFileScope::Recursive
        } else {
            crate::folder_scanner::ReleaseFileScope::Direct
        }),
        content_hash: None,
        file_edit_revision: 0,
        grouping_key: candidate.grouping.as_ref(),
        source_kind,
        blank_cover_file: candidate.cover_file(),
        invalid_reason: Some(reason),
        invalid_file_name: file_name,
        invalid_damaged_audio: damaged_audio,
        invalid_audio_bits: audio_bits,
    })?;
    for (position, file) in candidate.files.iter().enumerate() {
        let columns = proposed_role_columns(file.role);
        sql.run(&InsertSkippedFileParams {
            watched_folder_path: &watched_folder_path,
            candidate_path: &key,
            position: to_i64(position as u64, "a skipped file's position")?,
            relative_path: &file.relative_path,
            absolute_path: coven::path_bytes(&file.file.path),
            size: to_i64(file.file.size, "a skipped file's size")?,
            modified_at_ns: file.file.modified_at_ns,
            role: columns.role,
            audio_damage: columns.audio_damage,
            audio_bits: columns.audio_bits,
            document_kind: columns.document_kind,
        })?;
    }
    if let Some(tags) = &candidate.tags {
        sql.run(&InsertSkippedTagsParams {
            watched_folder_path: &watched_folder_path,
            candidate_path: &key,
            album_title: &tags.album_title,
            credits: tags
                .artist
                .as_ref()
                .map(crate::tables::stored_credits::line_json)
                .transpose()?,
        })?;
    }
    Ok(())
}

coven::write!(
    InsertSheetAudioFile(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        sheet_relative_path: &'p str,
        position: i64,
        file_reference: &'p str,
        audio_relative_path: &'p str,
    ),
    r#"
    INSERT INTO scan_sheet_audio_file
        (watched_folder_path, candidate_path, sheet_relative_path, position, file_reference,
         audio_relative_path)
    VALUES (:watched_folder_path, :candidate_path, :sheet_relative_path, :position,
        :file_reference, :audio_relative_path)
"#
);

coven::write!(
    SetBlankCoverFile(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        blank_cover_file: Option<String>,
    ),
    r#"
    UPDATE scan_candidate SET blank_cover_file = :blank_cover_file
    WHERE watched_folder_path = :watched_folder_path AND path = :candidate_path
"#
);

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
            sql.run(&InsertSheetAudioFileParams {
                watched_folder_path: &coven::path_bytes(watched_folder_path),
                candidate_path,
                sheet_relative_path: &sheet.file.relative_path,
                position: to_i64(position as u64, "a sheet audio file's position")?,
                file_reference: &audio.file_reference,
                audio_relative_path: &audio.file_id,
            })?;
        }
    }
    sql.run(&SetBlankCoverFileParams {
        watched_folder_path: &coven::path_bytes(watched_folder_path),
        candidate_path,
        blank_cover_file: crate::cover_art::blank_cover_file(files.artwork()),
    })?;
    super::units::insert_units(sql, watched_folder_path, candidate_path, files)
}

coven::write!(
    InsertCandidateFile(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        relative_path: &'p str,
        position: i64,
        absolute_path: Vec<u8>,
        size: i64,
        modified_at_ns: i64,
        audio_content_type: Option<&'p str>,
        audio_container: Option<&'p str>,
        audio_duration_ms: Option<i64>,
        audio_sample_rate_hz: Option<i64>,
        audio_bits_per_sample: Option<i64>,
        audio_bitrate_kbps: Option<i64>,
        audio_channels: Option<i64>,
        file_name: &'p str,
        role: &'static str,
        document_kind: Option<&'static str>,
        sheet_binding: Option<&'static str>,
        sheet_binding_codec: Option<&'p str>,
        sheet_place: Option<&'static str>,
        sheet_medium: Option<u32>,
        sheet_side: Option<u32>,
    ),
    r#"
    INSERT INTO scan_candidate_file
        (watched_folder_path, candidate_path, relative_path, position, absolute_path, size,
         modified_at_ns, audio_content_type, audio_container, audio_duration_ms,
         audio_sample_rate_hz, audio_bits_per_sample, audio_bitrate_kbps, audio_channels,
         file_name, role, document_kind, sheet_binding, sheet_binding_codec, sheet_place,
         sheet_medium, sheet_side)
    VALUES (:watched_folder_path, :candidate_path, :relative_path, :position, :absolute_path,
        :size, :modified_at_ns, :audio_content_type, :audio_container, :audio_duration_ms,
        :audio_sample_rate_hz, :audio_bits_per_sample, :audio_bitrate_kbps, :audio_channels,
        :file_name, :role, :document_kind, :sheet_binding, :sheet_binding_codec, :sheet_place,
        :sheet_medium, :sheet_side)
"#
);

fn insert_file(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    position: usize,
    file: &CandidateFile,
) -> Result<(), DbError> {
    let columns = role_columns(&file.role);
    let audio = file.file.source_audio.as_ref();
    sql.run(&InsertCandidateFileParams {
        watched_folder_path: &coven::path_bytes(watched_folder_path),
        candidate_path,
        relative_path: &file.file.relative_path,
        position: to_i64(position as u64, "a candidate file's position")?,
        absolute_path: coven::path_bytes(&file.file.version.path),
        size: to_i64(file.file.version.size, "a file's size")?,
        modified_at_ns: file.file.version.modified_at_ns,
        audio_content_type: audio.map(|audio| audio.content_type.as_str()),
        audio_container: audio.map(|audio| audio.container.as_str()),
        audio_duration_ms: audio
            .map(|audio| to_i64(audio.duration_ms, "an audio file's duration"))
            .transpose()?,
        audio_sample_rate_hz: audio.map(|audio| audio.format.sample_rate_hz),
        audio_bits_per_sample: audio.and_then(|audio| audio.format.bits_per_sample),
        audio_bitrate_kbps: audio.and_then(|audio| audio.format.bitrate_kbps),
        audio_channels: audio.map(|audio| audio.format.channels),
        file_name: &file.file.file_name,
        role: columns.role,
        document_kind: columns.document_kind,
        sheet_binding: columns.sheet_binding,
        sheet_binding_codec: columns.sheet_binding_codec,
        sheet_place: columns.sheet_place,
        sheet_medium: columns.sheet_medium,
        sheet_side: columns.sheet_side,
    })?;
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

coven::write!(
    InsertCueSheet(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        sheet_relative_path: &'p str,
        title: Option<&'p str>,
        performer: Option<&'p str>,
        catalog: Option<&'p str>,
        date: Option<&'p str>,
        ripper: Option<&'static str>,
    ),
    r#"
    INSERT INTO scan_cue_sheet
        (watched_folder_path, candidate_path, sheet_relative_path, title, performer, catalog,
         date, ripper)
    VALUES (:watched_folder_path, :candidate_path, :sheet_relative_path, :title, :performer,
        :catalog, :date, :ripper)
"#
);

coven::write!(
    InsertCueTrack(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        sheet_relative_path: &'p str,
        position: i64,
        number: u32,
        mode: &'static str,
        mode_other: Option<&'p str>,
        title: Option<&'p str>,
        performer: Option<&'p str>,
        file_reference: &'p str,
        start_cue_frames: i64,
        end_cue_frames: Option<i64>,
        generated_pregap_frames: Option<i64>,
    ),
    r#"
    INSERT INTO scan_cue_track
        (watched_folder_path, candidate_path, sheet_relative_path, position, number, mode,
         mode_other, title, performer, file_reference, start_cue_frames, end_cue_frames,
         generated_pregap_frames)
    VALUES (:watched_folder_path, :candidate_path, :sheet_relative_path, :position, :number,
        :mode, :mode_other, :title, :performer, :file_reference, :start_cue_frames,
        :end_cue_frames, :generated_pregap_frames)
"#
);

coven::write!(
    InsertCueIndex(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        sheet_relative_path: &'p str,
        track_position: i64,
        position: i64,
        number: u32,
        frames: i64,
        file_reference: &'p str,
    ),
    r#"
    INSERT INTO scan_cue_index
        (watched_folder_path, candidate_path, sheet_relative_path, track_position, position,
         number, frames, file_reference)
    VALUES (:watched_folder_path, :candidate_path, :sheet_relative_path, :track_position,
        :position, :number, :frames, :file_reference)
"#
);

fn insert_cue_sheet(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    sheet_relative_path: &str,
    sheet: &CueSheet,
) -> Result<(), DbError> {
    let watched_folder_path = coven::path_bytes(watched_folder_path);
    sql.run(&InsertCueSheetParams {
        watched_folder_path: &watched_folder_path,
        candidate_path,
        sheet_relative_path,
        title: sheet.title.as_deref(),
        performer: sheet.performer.as_deref(),
        catalog: sheet.catalog.as_deref(),
        date: sheet.date.as_deref(),
        ripper: sheet.ripper.map(ripper_text),
    })?;
    for (position, track) in sheet.tracks.iter().enumerate() {
        let position = to_i64(position as u64, "a cue track's position")?;
        let (mode, mode_other) = match &track.mode {
            crate::cue_sheet::CueTrackMode::Audio => ("audio", None),
            crate::cue_sheet::CueTrackMode::Other(other) => ("other", Some(other.as_str())),
        };
        sql.run(&InsertCueTrackParams {
            watched_folder_path: &watched_folder_path,
            candidate_path,
            sheet_relative_path,
            position,
            number: track.number,
            mode,
            mode_other,
            title: track.title.as_deref(),
            performer: track.performer.as_deref(),
            file_reference: &track.file_reference,
            start_cue_frames: to_i64(track.start_cue_frames, "a cue track's start")?,
            end_cue_frames: track
                .end_cue_frames
                .map(|frames| to_i64(frames, "a cue track's end"))
                .transpose()?,
            generated_pregap_frames: track
                .generated_pregap_frames
                .map(|frames| to_i64(frames, "a generated pregap's length"))
                .transpose()?,
        })?;
        for (index_position, index) in track.indexes.iter().enumerate() {
            sql.run(&InsertCueIndexParams {
                watched_folder_path: &watched_folder_path,
                candidate_path,
                sheet_relative_path,
                track_position: position,
                position: to_i64(index_position as u64, "a cue index's position")?,
                number: index.number,
                frames: to_i64(index.frames, "a cue index's frame position")?,
                file_reference: &index.file_reference,
            })?;
        }
    }
    Ok(())
}

// Damage a test plants: every candidate stamped with `generation`.
#[cfg(test)]
coven::write!(
    StampEveryCandidate(generation: i64),
    r#"UPDATE scan_candidate SET generation = :generation"#
);

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
