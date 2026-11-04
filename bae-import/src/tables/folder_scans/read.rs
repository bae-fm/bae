//! Reading scan items back out of the rows [`super::write`] laid down.
//!
//! One query for every entry a read takes in: each entry's row carries what
//! hangs off it — its files with their sheets, its parts, and a failed
//! release's skipped files and tags — as JSON beside its own columns. Owned
//! finishing callbacks let reads defer decoding them until the connection is
//! released, while write decisions finish it inside their transaction. The
//! boxed callbacks erase the generic SQL implementation and its lifetime at
//! the read boundary.

use super::columns::*;
use super::*;
use crate::cue_flac::{CueIndex, CueSheet, CueTrack, CueTrackMode};
use crate::folder_scanner::{
    CandidateFile, CategorizedFiles, Coverage, FileRole, FileVersion, FolderCandidate,
    InvalidCandidate, ReleasePart, ScanItem, ScannedAudio, ScannedFile, SheetAudioFile,
    SkippedFile, SkippedTags,
};
use crate::tables::FinishRead;
use crate::CandidateKey;
use bae_db::RelativeFolder;
use bae_types::audio_format::AudioFormat;
use bae_util::content_type::ContentType;
use track_sheets::{StoredSheet, StoredSheetAudio, SHEET_AUDIO_JSON, SHEET_JSON};

mod track_sheets;

/// Which rows a read takes in: what the scans wrote, or those and the
/// releases groupings built from them too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowSources {
    Scanned,
    Any,
}

impl RowSources {
    /// The condition keeping these rows of `scan_candidate`, aliased
    /// `candidate`.
    fn clause(self) -> &'static str {
        match self {
            Self::Scanned => "candidate.source_kind = 'folder'",
            Self::Any => "1",
        }
    }
}

/// Whose rows a read takes in: every entry's under a root, or the one entry's
/// at a key, which each table's primary key reaches directly.
#[derive(Debug, Clone, Copy)]
pub(crate) enum EntryRows<'a> {
    Root(&'a Path),
    Entry { root: &'a Path, key: &'a CandidateKey },
}

impl<'a> EntryRows<'a> {
    fn root(self) -> &'a Path {
        match self {
            Self::Root(root) | Self::Entry { root, .. } => root,
        }
    }

    /// The condition keeping these entries of `scan_candidate`, aliased
    /// `candidate`, binding the root as `?1` and the key as `?2`.
    fn condition(self) -> &'static str {
        match self {
            Self::Root(_) => "candidate.watched_folder_path = ?1",
            Self::Entry { .. } => "candidate.watched_folder_path = ?1 AND candidate.path = ?2",
        }
    }

    /// What [`Self::condition`] binds.
    fn params(self) -> rusqlite::ParamsFromIter<Vec<Vec<u8>>> {
        rusqlite::params_from_iter(match self {
            Self::Root(root) => vec![coven::path_bytes(root)],
            Self::Entry { root, key } => vec![coven::path_bytes(root), key.as_bytes().to_vec()],
        })
    }
}

/// One stored entry: the key it is addressed by, the scan generation that
/// wrote it, and the item itself.
pub(crate) struct StoredScanItem {
    pub(crate) key: CandidateKey,
    pub(crate) generation: u64,
    pub(crate) item: ScanItem,
}

/// Every entry stored under one watched root, in persisted-key order.
///
/// An entry stamped with a generation the root never reached is a store
/// nothing here wrote, so the assembly refuses it.
pub(super) fn load_items(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &Path,
    sources: RowSources,
) -> Result<FinishRead<Vec<StoredScanItem>>, DbError> {
    let items = load_candidate_items_rows(sql, EntryRows::Root(watched_folder_path), sources)?;
    let root_generation = super::root_generation_on(sql, watched_folder_path)?
        .map(|generation| to_u64(generation, "a folder scan root's generation"))
        .transpose()?;
    Ok(Box::new(move || {
        let items = items()?;
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
    }))
}

/// The entry at `entry_key`, and the root it is under. Watched roots never
/// overlap and keys are absolute paths, so at most one root holds it.
pub(crate) fn load_item_by_key(
    sql: &(impl QueryOne + QueryRows),
    entry_key: &CandidateKey,
    sources: RowSources,
) -> Result<Option<(PathBuf, StoredScanItem)>, DbError> {
    load_item_by_key_rows(sql, entry_key, sources)?.map(|process| process()).transpose()
}

pub(crate) fn load_item_by_key_rows(
    sql: &(impl QueryOne + QueryRows),
    entry_key: &CandidateKey,
    sources: RowSources,
) -> Result<Option<FinishRead<(PathBuf, StoredScanItem)>>, DbError> {
    let roots = sql.query(
        &format!(
            "SELECT candidate.watched_folder_path FROM scan_candidate AS candidate \
             WHERE candidate.path = ? AND {}",
            sources.clause()
        ),
        [entry_key],
        |row| path_column::get(row, 0),
    )?;
    if roots.len() > 1 {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} is stored under {} roots",
            roots.len()
        )));
    }
    if let Some(root) = roots.into_iter().next() {
        let items = load_candidate_items_rows(
            sql,
            EntryRows::Entry { root: &root, key: entry_key },
            sources,
        )?;
        let entry_key = entry_key.clone();
        return Ok(Some(Box::new(move || {
            let mut items = items()?;
            match items.pop() {
                Some(item) => Ok((root, item)),
                None => Err(DbError::Message(format!(
                    "folder scan candidate {entry_key} vanished between its two reads"
                ))),
            }
        })));
    }
    Ok(None)
}

/// Whether a valid or invalid entry, rather than a tentative one or none, is
/// stored at `path`.
pub(crate) fn settled_entry_is_stored(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &Path,
    path: &CandidateKey,
) -> Result<bool, DbError> {
    let kind: Option<String> = sql
        .query_row(
            "SELECT kind FROM scan_candidate WHERE watched_folder_path = ? AND path = ?",
            params![coven::path_bytes(watched_folder_path), path],
            |row| row.get(0),
        )
        .optional()?;
    Ok(matches!(kind.as_deref(), Some("valid" | "invalid")))
}

/// Which of the entries reading a folder's files a write asks after.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Readers<'a> {
    /// The settled ones, valid or invalid: what a tentative candidate stands
    /// down for.
    Settled,
    /// Every one but the entry at this key: what a settled write replaces.
    Besides(&'a CandidateKey),
}

/// The keys of the entries the scans stored under the root that read any of
/// the files `coverage` reads, in key order: an entry whose folder is
/// `coverage`'s, one above it read whole, or — for `coverage` over a whole
/// folder — one below it.
///
/// Each of those is a lookup by folder in `idx_scan_candidate_file_root`.
/// With statistics on `scan_candidate` SQLite picks that index itself. The
/// `+` is for a library whose `scan_candidate` has never been analyzed — its
/// first scan, before coven's `PRAGMA optimize` at the next open or close
/// gathers them: without statistics SQLite takes a root to hold few rows and
/// would read every one of them in key order instead.
pub(crate) fn entries_reading(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &Path,
    coverage: &Coverage,
    readers: Readers<'_>,
) -> Result<Vec<CandidateKey>, DbError> {
    let root = coven::path_bytes(watched_folder_path);
    let folder = coven::path_bytes(&coverage.folder);
    let above = path_column::hex_list(
        coverage
            .folder
            .ancestors()
            .skip(1)
            .take_while(|above| above.starts_with(watched_folder_path)),
    );
    let below = coverage.whole_subtree.then(|| bae_db::watched_path::paths_below(&coverage.folder));
    let mut params: Vec<(&str, &dyn rusqlite::ToSql)> =
        vec![(":root", &root), (":folder", &folder), (":above", &above)];
    let readers_clause = match &readers {
        Readers::Settled => "kind IN ('valid', 'invalid')",
        Readers::Besides(key) => {
            params.push((":key", key));
            "path != :key"
        }
    };
    let below_clause = match &below {
        Some((first, past)) => {
            params.push((":below_first", first));
            params.push((":below_past", past));
            " OR (file_root >= :below_first AND file_root < :below_past)"
        }
        None => "",
    };
    Ok(sql.query(
        &format!(
            "SELECT path FROM scan_candidate \
             WHERE +watched_folder_path = :root AND source_kind = 'folder' \
               AND {readers_clause} \
               AND (file_root = :folder \
                    OR (scope = 'recursive' \
                        AND file_root IN (SELECT unhex(value) FROM json_each(:above))){below_clause}) \
             ORDER BY path"
        ),
        params.as_slice(),
        |row| row.get::<_, CandidateKey>(0),
    )?)
}

// ── Candidates ──────────────────────────────────────────────────────────────

/// The scan generation the valid candidate at `candidate_path` under
/// `watched_folder_path` is stored at, refused unless the scan still lists it
/// with the files `content_hash` names at `expected_file_edit_revision`: what a
/// write prepared against those files checks before it lands.
pub(crate) fn require_current_candidate(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    content_hash: &str,
    expected_file_edit_revision: u64,
) -> Result<u64, DbError> {
    let current: Option<(String, i64, i64)> = sql
        .query_row(
            "SELECT content_hash, generation, file_edit_revision \
             FROM scan_candidate WHERE watched_folder_path = ? AND path = ? AND kind = 'valid'",
            params![coven::path_bytes(watched_folder_path), candidate_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let expected_file_edit_revision =
        to_i64(expected_file_edit_revision, "candidate file revision")?;
    let Some((current_hash, generation, current_file_edit_revision)) = current else {
        return Err(DbError::Message(format!(
            "candidate changed before metadata was stored: {candidate_path} is no longer valid"
        )));
    };
    if current_hash != content_hash || current_file_edit_revision != expected_file_edit_revision {
        return Err(DbError::Message(format!(
            "candidate changed before metadata was stored: {candidate_path} no longer names the prepared files"
        )));
    }
    to_u64(generation, "scan_candidate.generation")
}

/// The files of the entry joined as `candidate`, as a JSON array in their
/// order: each with its sheet and the audio that sheet describes.
fn files_json_sql() -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'relative_path', file.relative_path, 'absolute_path', hex(file.absolute_path), \
             'size', file.size, 'modified_at_ns', file.modified_at_ns, \
             'audio_content_type', file.audio_content_type, \
             'audio_duration_ms', file.audio_duration_ms, \
             'audio_sample_rate_hz', file.audio_sample_rate_hz, \
             'audio_bits_per_sample', file.audio_bits_per_sample, \
             'audio_bitrate_kbps', file.audio_bitrate_kbps, \
             'audio_channels', file.audio_channels, 'file_name', file.file_name, \
             'dir_prefix', file.dir_prefix, 'role', file.role, \
             'sheet_binding', file.sheet_binding, \
             'sheet_binding_codec', file.sheet_binding_codec, \
             'sheet_disc', file.sheet_disc, 'sheet_disc_number', file.sheet_disc_number, \
             'sheet', json({SHEET_JSON}), 'sheet_audio', json({SHEET_AUDIO_JSON})) \
             ORDER BY file.position) \
         FROM scan_candidate_file AS file \
         WHERE file.watched_folder_path = candidate.watched_folder_path \
           AND file.candidate_path = candidate.path)"
    )
}

/// The parts the entry joined as `candidate` is read from, as a JSON array
/// in play order.
const PARTS_JSON: &str = "(SELECT json_group_array(json_object( \
         'folder', hex(part.folder), 'prefix', part.prefix) ORDER BY part.position) \
     FROM scan_candidate_part AS part \
     WHERE part.watched_folder_path = candidate.watched_folder_path \
       AND part.candidate_path = candidate.path)";

/// The files of the release joined as `candidate` that failed validation, as
/// a JSON array in name order.
const SKIPPED_FILES_JSON: &str = "(SELECT json_group_array(json_object( \
         'relative_path', skipped.relative_path, \
         'absolute_path', hex(skipped.absolute_path), \
         'size', skipped.size, 'modified_at_ns', skipped.modified_at_ns, \
         'role', skipped.role, 'audio_damage', skipped.audio_damage) ORDER BY skipped.position) \
     FROM scan_skipped_file AS skipped \
     WHERE skipped.watched_folder_path = candidate.watched_folder_path \
       AND skipped.candidate_path = candidate.path)";

struct CandidateRow {
    path: CandidateKey,
    generation: i64,
    kind: String,
    name: String,
    relative_folder: RelativeFolder,
    folder: PathBuf,
    file_root: PathBuf,
    scope: String,
    file_edit_revision: i64,
    grouping_key: Option<CandidateKey>,
    files: String,
    parts: String,
    skipped_files: String,
    /// What the tags of a release that failed validation call it, when they
    /// name an album.
    skipped_tags: Option<SkippedTags>,
}

/// The entries `entries` names, each read with what hangs off it.
pub(crate) fn load_candidate_items_rows(
    sql: &(impl QueryOne + QueryRows),
    entries: EntryRows<'_>,
    sources: RowSources,
) -> Result<FinishRead<Vec<StoredScanItem>>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT candidate.path, candidate.generation, candidate.kind, candidate.name, \
                    candidate.relative_folder, candidate.folder, candidate.file_root, \
                    candidate.scope, candidate.file_edit_revision, candidate.grouping_key, \
                    {files} AS files, {PARTS_JSON} AS parts, \
                    {SKIPPED_FILES_JSON} AS skipped_files, \
                    tags.album_title AS skipped_album_title, tags.artist AS skipped_artist \
             FROM scan_candidate AS candidate \
             LEFT JOIN scan_skipped_tags AS tags \
                 ON tags.watched_folder_path = candidate.watched_folder_path \
                AND tags.candidate_path = candidate.path \
             WHERE {} AND {} \
             ORDER BY candidate.path",
            entries.condition(),
            sources.clause(),
            files = files_json_sql(),
        ),
        entries.params(),
        |row| {
            Ok(CandidateRow {
                path: row.get("path")?,
                generation: row.get("generation")?,
                kind: row.get("kind")?,
                name: row.get("name")?,
                relative_folder: row.get("relative_folder")?,
                folder: path_column::get(row, "folder")?,
                file_root: path_column::get(row, "file_root")?,
                scope: row.get("scope")?,
                file_edit_revision: row.get("file_edit_revision")?,
                grouping_key: row.get("grouping_key")?,
                files: row.get("files")?,
                parts: row.get("parts")?,
                skipped_files: row.get("skipped_files")?,
                skipped_tags: row
                    .get::<_, Option<String>>("skipped_album_title")?
                    .map(|album_title| {
                        Ok::<_, rusqlite::Error>(SkippedTags {
                            album_title,
                            artist: row.get("skipped_artist")?,
                        })
                    })
                    .transpose()?,
            })
        },
    )?;
    let watched_folder_path = entries.root().to_path_buf();
    Ok(Box::new(move || {
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let generation = to_u64(row.generation, "a scan candidate's generation")?;
            let item = match row.kind.as_str() {
                "invalid" => ScanItem::Invalid(InvalidCandidate {
                    path: row.folder,
                    name: row.name,
                    watched_folder_path: watched_folder_path.clone(),
                    relative_folder: row.relative_folder,
                    grouping: row.grouping_key,
                    files: skipped_files_of(&row.path, &row.skipped_files)?,
                    tags: row.skipped_tags,
                }),
                kind @ ("tentative" | "valid") => {
                    let candidate = FolderCandidate {
                        path: row.folder,
                        file_root: row.file_root,
                        name: row.name,
                        files: CategorizedFiles {
                            files: files_of(&row.path, &row.files)?,
                            parts: parts_of(&row.path, &row.parts)?,
                        },
                        watched_folder_path: watched_folder_path.clone(),
                        scope: scope_of(&row.scope)?,
                        file_edit_revision: to_u64(
                            row.file_edit_revision,
                            "a scan candidate's file edit revision",
                        )?,
                        relative_folder: row.relative_folder,
                        grouping: row.grouping_key,
                    };
                    match kind {
                        "tentative" => ScanItem::Discovered(candidate),
                        _ => ScanItem::Valid(candidate),
                    }
                }
                other => return Err(DbError::unreadable("scan_candidate.kind", other)),
            };
            items.push(StoredScanItem { key: row.path, generation, item });
        }
        Ok(items)
    }))
}

/// A list an entry's row holds as JSON, decoded.
fn listed<T: serde::de::DeserializeOwned>(
    candidate_path: &CandidateKey,
    json: &str,
    what: &str,
) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json).map_err(|error| {
        DbError::Message(format!("the {what} stored under {candidate_path} do not read: {error}"))
    })
}

/// One `scan_candidate_file` row as [`files_json_sql`] holds it.
#[derive(serde::Deserialize)]
struct FileRow {
    relative_path: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    absolute_path: PathBuf,
    size: i64,
    modified_at_ns: i64,
    audio_content_type: Option<String>,
    audio_duration_ms: Option<i64>,
    audio_sample_rate_hz: Option<i64>,
    audio_bits_per_sample: Option<i64>,
    audio_bitrate_kbps: Option<i64>,
    audio_channels: Option<i64>,
    file_name: String,
    dir_prefix: Option<String>,
    role: String,
    sheet_binding: Option<String>,
    sheet_binding_codec: Option<String>,
    sheet_disc: Option<String>,
    sheet_disc_number: Option<i64>,
    sheet: Option<StoredSheet>,
    sheet_audio: Vec<StoredSheetAudio>,
}

/// The files of the entry at `candidate_path`, in their order.
fn files_of(candidate_path: &CandidateKey, json: &str) -> Result<Vec<CandidateFile>, DbError> {
    let files = listed::<FileRow>(candidate_path, json, "files")?
        .into_iter()
        .map(|row| {
            let source_audio = source_audio_of(&row)?;
            let role = match row.role.as_str() {
                "audio" => FileRole::Audio,
                "artwork" => FileRole::Artwork,
                "damaged_image" => FileRole::DamagedImage,
                "document" => FileRole::Document,
                "other" => FileRole::Other,
                "track_sheet" => {
                    let missing = |what: &str| {
                        DbError::Message(format!(
                            "track sheet {} under {candidate_path} has no {what}",
                            row.relative_path
                        ))
                    };
                    FileRole::TrackSheet {
                        sheet: row.sheet.ok_or_else(|| missing("parsed sheet"))?.sheet()?,
                        binding: sheet_binding_of(
                            &row.sheet_binding.ok_or_else(|| missing("binding"))?,
                            row.sheet_audio.into_iter().map(StoredSheetAudio::audio_file).collect(),
                            row.sheet_binding_codec,
                        )?,
                        disc: sheet_disc_of(
                            &row.sheet_disc.ok_or_else(|| missing("disc"))?,
                            row.sheet_disc_number,
                        )?,
                    }
                }
                other => return Err(DbError::unreadable("scan_candidate_file.role", other)),
            };
            Ok(CandidateFile {
                file: ScannedFile {
                    path: row.absolute_path,
                    relative_path: row.relative_path,
                    size: to_u64(row.size, "a file's size")?,
                    modified_at_ns: row.modified_at_ns,
                    dir_prefix: row.dir_prefix,
                    file_name: row.file_name,
                    source_audio,
                },
                role,
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    check_sheet_bindings_name_audio(candidate_path, &files)?;
    Ok(files)
}
/// A binding is settled against the folder's audio, so every file it names
/// has the audio role. The table can only say the file exists; this says it
/// is audio, so a candidate that violates it is refused here rather than
/// panicking whoever reads the bound sheets.
fn check_sheet_bindings_name_audio(
    candidate_path: &CandidateKey,
    files: &[CandidateFile],
) -> Result<(), DbError> {
    let audio: std::collections::HashSet<&str> = files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Audio))
        .map(|entry| entry.file.relative_path.as_str())
        .collect();
    for entry in files {
        let FileRole::TrackSheet { binding, .. } = &entry.role else {
            continue;
        };
        for named in binding.audio_files().unwrap_or_default() {
            if !audio.contains(named.file_id.as_str()) {
                return Err(DbError::Message(format!(
                    "track sheet {} under {candidate_path} describes {}, which is not the \
                     candidate's audio",
                    entry.file.relative_path, named.file_id
                )));
            }
        }
    }
    Ok(())
}

fn source_audio_of(row: &FileRow) -> Result<Option<ScannedAudio>, DbError> {
    match (
        row.audio_content_type.as_deref(),
        row.audio_duration_ms,
        row.audio_sample_rate_hz,
        row.audio_bits_per_sample,
        row.audio_bitrate_kbps,
        row.audio_channels,
    ) {
        (None, None, None, None, None, None) if row.role != "audio" => Ok(None),
        (
            Some(content_type),
            Some(duration_ms),
            Some(sample_rate_hz),
            bits_per_sample,
            bitrate_kbps,
            Some(channels),
        ) if row.role == "audio" => {
            let content_type = ContentType::from_mime(content_type);
            if !content_type.is_audio() {
                return Err(DbError::Message(format!(
                    "candidate audio file {} stores non-audio content type {}",
                    row.relative_path,
                    content_type.as_str()
                )));
            }
            to_u64(sample_rate_hz, "an audio file's sample rate")?;
            bits_per_sample.map(|value| to_u64(value, "an audio file's bit depth")).transpose()?;
            bitrate_kbps.map(|value| to_u64(value, "an audio file's bitrate")).transpose()?;
            to_u64(channels, "an audio file's channel count")?;
            Ok(Some(ScannedAudio {
                format: AudioFormat {
                    codec: content_type.display_name().to_string(),
                    sample_rate_hz,
                    bits_per_sample,
                    bitrate_kbps,
                    channels,
                },
                content_type,
                duration_ms: to_u64(duration_ms, "an audio file's duration")?,
            }))
        }
        columns => Err(DbError::Message(format!(
            "candidate file {} stores inconsistent source-audio facts: {columns:?}",
            row.relative_path
        ))),
    }
}

/// One `scan_skipped_file` row as [`SKIPPED_FILES_JSON`] holds it.
#[derive(serde::Deserialize)]
struct SkippedFileRow {
    relative_path: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    absolute_path: PathBuf,
    size: i64,
    modified_at_ns: i64,
    role: String,
    audio_damage: Option<String>,
}

/// The files of the release at `candidate_path` that failed validation.
fn skipped_files_of(
    candidate_path: &CandidateKey,
    json: &str,
) -> Result<Vec<SkippedFile>, DbError> {
    listed::<SkippedFileRow>(candidate_path, json, "skipped files")?
        .into_iter()
        .map(|row| {
            Ok(SkippedFile {
                relative_path: row.relative_path,
                file: FileVersion {
                    path: row.absolute_path,
                    size: to_u64(row.size, "a skipped file's size")?,
                    modified_at_ns: row.modified_at_ns,
                },
                role: proposed_role_of(&row.role, row.audio_damage.as_deref())?,
            })
        })
        .collect()
}

/// One `scan_candidate_part` row as [`PARTS_JSON`] holds it.
#[derive(serde::Deserialize)]
struct PartRow {
    #[serde(deserialize_with = "path_column::from_hex")]
    folder: PathBuf,
    prefix: String,
}

/// The parts the release at `candidate_path` is read from, in play order.
fn parts_of(candidate_path: &CandidateKey, json: &str) -> Result<Vec<ReleasePart>, DbError> {
    Ok(listed::<PartRow>(candidate_path, json, "parts")?
        .into_iter()
        .map(|part| ReleasePart { folder: part.folder, prefix: part.prefix })
        .collect())
}
