//! Reading scan items back out of the rows [`super::write`] laid down.
//!
//! One query for every entry a read takes in:
//! [`super::fragments::ScanItems`] over their keys, whose row carries what
//! hangs off an entry — its files with their sheets, its parts, and a failed
//! release's skipped files and tags — as JSON beside its own columns. Owned
//! finishing callbacks let reads defer decoding them until the connection is
//! released, while write decisions finish it inside their transaction. The
//! boxed callbacks erase the generic SQL implementation and its lifetime at
//! the read boundary.

use super::columns::*;
use super::*;
use crate::cue_sheet::{CueIndex, CueSheet, CueTrack, CueTrackMode};
use crate::folder_scanner::{
    CandidateFile, CategorizedFiles, Coverage, FileRole, FileVersion, FolderCandidate,
    InvalidCandidate, ReleasePart, ScanItem, ScannedAudio, ScannedFile, SheetAudioFile, SheetPlace,
    SkippedFile,
};
use crate::tables::FinishRead;
use crate::CandidateKey;
use crate::RelativeFolder;
use bae_types::audio_format::AudioFormat;
use bae_util::audio_container::AudioContainer;
use bae_util::content_type::ContentType;
use track_sheets::{StoredSheet, StoredSheetAudio};

mod track_sheets;

/// Which rows a read takes in: what the scans wrote, or those and the
/// releases groupings built from them too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowSources {
    Scanned,
    Any,
}

impl RowSources {
    /// Whether these rows take in the releases groupings built.
    fn takes_grouping_releases(self) -> bool {
        match self {
            Self::Scanned => false,
            Self::Any => true,
        }
    }
}

sqlive_compose::read!(
    writer ItemsAtKeys(keys: String, grouping_releases: bool),
    r#"
    SELECT item.*
    FROM @crate::tables::folder_scans::fragments::ScanItems(keys: (
        SELECT unhex(wanted.value) AS candidate_key FROM json_each(:keys) AS wanted)) AS item
    WHERE :grouping_releases OR item.source_kind = 'folder'
    ORDER BY item.path
"#
);

/// The rows of the entries at `keys`, in key order.
fn rows_at_keys<'a>(
    sql: &impl bae_store::WriterRead,
    keys: impl Iterator<Item = &'a CandidateKey>,
    sources: RowSources,
) -> Result<Vec<CandidateRow>, DbError> {
    let statement = ItemsAtKeysParams {
        keys: crate::tables::key_hex_list(keys),
        grouping_releases: sources.takes_grouping_releases(),
    };
    statement_rows(sql, &statement)?.into_iter().map(|row| CandidateRow::of(row.item)).collect()
}

/// One stored entry: the key it is addressed by, the scan generation that
/// wrote it, and the item itself.
pub(crate) struct StoredScanItem {
    pub(crate) key: CandidateKey,
    pub(crate) generation: u64,
    pub(crate) item: ScanItem,
}

#[cfg(any(test, feature = "test-utils"))]
sqlive_compose::read!(
    writer ItemsUnderRoot(root: Vec<u8>, grouping_releases: bool),
    r#"
    SELECT item.*
    FROM @crate::tables::folder_scans::fragments::ScanItems(keys: (
        SELECT candidate.path AS candidate_key
        FROM scan_candidate AS candidate
        WHERE candidate.watched_folder_path = :root)) AS item
    WHERE item.watched_folder_path = :root
        AND (:grouping_releases OR item.source_kind = 'folder')
    ORDER BY item.path
"#
);

/// Every entry stored under one watched root, in key order, as a test reads
/// them.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn load_items(
    sql: &impl bae_store::WriterRead,
    watched_folder_path: &Path,
    sources: RowSources,
) -> Result<FinishRead<Vec<StoredScanItem>>, DbError> {
    let request = ItemsUnderRootParams {
        root: coven::path_bytes(watched_folder_path),
        grouping_releases: sources.takes_grouping_releases(),
    };
    let rows = statement_rows(sql, &request)?
        .into_iter()
        .map(|row| CandidateRow::of(row.item))
        .collect::<Result<Vec<_>, _>>()?;
    let root_generation = super::root_generation_on(sql, watched_folder_path)?;
    Ok(Box::new(move || {
        let items =
            rows.into_iter().map(CandidateRow::item).collect::<Result<Vec<_>, DbError>>()?;
        check_generations(&items, root_generation)?;
        Ok(items)
    }))
}

/// Refuse an entry of `items` stamped with a generation their root, at
/// `root_generation`, never reached.
pub(super) fn check_generations(
    items: &[StoredScanItem],
    root_generation: Option<i64>,
) -> Result<(), DbError> {
    let Some(root_generation) = root_generation else {
        return Ok(());
    };
    let root_generation = to_u64(root_generation, "a folder scan root's generation")?;
    match items.iter().find(|item| item.generation > root_generation) {
        Some(item) => Err(DbError::unreadable(
            format!("scan_candidate.generation of {}, newer than its root's", item.key),
            (item.generation, root_generation),
        )),
        None => Ok(()),
    }
}

/// The entry at `entry_key`, and the root it is under. Watched roots never
/// overlap and keys are absolute paths, so at most one root holds it.
pub(crate) fn load_item_by_key(
    sql: &impl bae_store::WriterRead,
    entry_key: &CandidateKey,
    sources: RowSources,
) -> Result<Option<(PathBuf, StoredScanItem)>, DbError> {
    load_item_by_key_rows(sql, entry_key, sources)?.map(|process| process()).transpose()
}

pub(crate) fn load_item_by_key_rows(
    sql: &impl bae_store::WriterRead,
    entry_key: &CandidateKey,
    sources: RowSources,
) -> Result<Option<FinishRead<(PathBuf, StoredScanItem)>>, DbError> {
    let mut rows = rows_at_keys(sql, std::iter::once(entry_key), sources)?;
    if rows.len() > 1 {
        return Err(DbError::unreadable(
            format!("the watched folders scan_candidate stores {entry_key} under"),
            rows.iter().map(|row| coven::path_text(row.root())).collect::<Vec<_>>(),
        ));
    }
    Ok(rows.pop().map(|row| -> FinishRead<(PathBuf, StoredScanItem)> {
        Box::new(move || {
            let root = row.root().to_path_buf();
            Ok((root, row.item()?))
        })
    }))
}

/// Whether a valid or invalid entry, rather than a tentative one or none, is
/// stored at `path`.
pub(crate) fn settled_entry_is_stored(
    sql: &impl bae_store::WriterRead,
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

/// The columns a row of [`super::fragments::FolderReaders`]' coverages binds:
/// the root, the folder the coverage reads, the folders above it up to the
/// root, and the range of folders below it when it reads them all.
pub(super) struct CoveredFolders {
    pub root: Vec<u8>,
    pub folder: Vec<u8>,
    /// A JSON array of the folders' paths in hex.
    pub above: String,
    pub below: Option<(Vec<u8>, Vec<u8>)>,
}

impl CoveredFolders {
    pub(super) fn of(watched_folder_path: &Path, coverage: &Coverage) -> Self {
        Self {
            root: coven::path_bytes(watched_folder_path),
            folder: coven::path_bytes(&coverage.folder),
            above: path_column::hex_list(
                coverage
                    .folder
                    .ancestors()
                    .skip(1)
                    .take_while(|above| above.starts_with(watched_folder_path)),
            ),
            below: coverage
                .whole_subtree
                .then(|| crate::tables::watched_path::paths_below(&coverage.folder)),
        }
    }
}

sqlive_compose::read!(
    writer ReadersCovering(
        root: Vec<u8>,
        folder: Vec<u8>,
        above: String,
        below_first: Option<Vec<u8>>,
        below_past: Option<Vec<u8>>,
        besides: Option<crate::CandidateKey>,
    ),
    r#"
    SELECT reader.*
    FROM @crate::tables::folder_scans::fragments::FolderReaders(covered: (
        SELECT 0 AS at, :root AS root, :folder AS folder, :above AS above,
            :below_first AS below_first, :below_past AS below_past, :besides AS besides)) AS reader
    ORDER BY +reader.reader_key
"#
);

/// The keys of the entries the scans stored under the root that read any of
/// the files `coverage` reads, in key order: an entry whose folder is
/// `coverage`'s, one above it read whole, or — for `coverage` over a whole
/// folder — one below it.
///
/// A scan asks this of every folder it writes, so it reads only the entries
/// it returns, whatever else the root holds: see
/// [`super::fragments::FolderReaders`].
pub(crate) fn entries_reading(
    sql: &impl bae_store::WriterRead,
    watched_folder_path: &Path,
    coverage: &Coverage,
    readers: Readers<'_>,
) -> Result<Vec<CandidateKey>, DbError> {
    let covered = CoveredFolders::of(watched_folder_path, coverage);
    let (below_first, below_past) = covered.below.unzip();
    let request = ReadersCoveringParams {
        root: covered.root,
        folder: covered.folder,
        above: covered.above,
        below_first,
        below_past,
        besides: match readers {
            Readers::Settled => None,
            Readers::Besides(key) => Some(key.clone()),
        },
    };
    Ok(statement_rows(sql, &request)?.into_iter().map(|row| row.reader.reader_key).collect())
}

// ── Candidates ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub(crate) struct CandidateRow {
    watched_folder_path: PathBuf,
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
    /// What a folder that failed validation stores: none for a row of
    /// another kind.
    invalid: Option<InvalidColumns>,
}

impl CandidateRow {
    /// The row of a read that spreads [`super::fragments::ScanItems`].
    pub(crate) fn of(scan: super::fragments::ScanItems) -> Result<Self, DbError> {
        let invalid = InvalidColumns::of(&scan)?;
        Ok(Self {
            watched_folder_path: path_column::decode(scan.watched_folder_path)?,
            path: scan.path,
            generation: scan.generation,
            kind: scan.kind,
            name: scan.name,
            relative_folder: scan.relative_folder,
            folder: path_column::decode(scan.folder)?,
            file_root: path_column::decode(scan.file_root)?,
            scope: scan.scope,
            file_edit_revision: scan.file_edit_revision,
            grouping_key: scan.grouping_key,
            files: scan.files,
            parts: scan.parts,
            skipped_files: scan.skipped_files,
            invalid,
        })
    }

    /// The root the entry is stored under.
    pub(crate) fn root(&self) -> &Path {
        &self.watched_folder_path
    }

    /// The stored entry the row holds.
    pub(crate) fn item(self) -> Result<StoredScanItem, DbError> {
        let row = self;
        let generation = to_u64(row.generation, "a scan candidate's generation")?;
        let item = match row.kind.as_str() {
            "invalid" => ScanItem::Invalid(InvalidCandidate {
                path: row.folder,
                name: row.name,
                watched_folder_path: row.watched_folder_path,
                relative_folder: row.relative_folder,
                grouping: row.grouping_key,
                files: skipped_files_of(&row.path, &row.skipped_files)?,
                // The schema holds a reason on every row of an invalid
                // folder.
                tags: row
                    .invalid
                    .ok_or_else(|| {
                        DbError::unreadable(
                            format!("the reason folder {} failed validation", row.path),
                            None::<String>,
                        )
                    })?
                    .tags(),
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
                    watched_folder_path: row.watched_folder_path,
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
        Ok(StoredScanItem { key: row.path, generation, item })
    }
}

/// A list an entry's row holds as JSON, decoded.
fn listed<T: serde::de::DeserializeOwned>(
    candidate_path: &CandidateKey,
    json: &str,
    what: &str,
) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json).map_err(|error| {
        DbError::unreadable_json(format!("the {what} stored under {candidate_path}"), json, error)
    })
}

/// One `scan_candidate_file` row as [`super::fragments::ScanItems`] holds it.
#[derive(serde::Deserialize)]
struct FileRow {
    relative_path: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    absolute_path: PathBuf,
    size: i64,
    modified_at_ns: i64,
    audio_content_type: Option<String>,
    audio_container: Option<String>,
    audio_duration_ms: Option<i64>,
    audio_sample_rate_hz: Option<i64>,
    audio_bits_per_sample: Option<i64>,
    audio_bitrate_kbps: Option<i64>,
    audio_channels: Option<i64>,
    file_name: String,
    role: String,
    document_kind: Option<String>,
    sheet_binding: Option<String>,
    sheet_binding_codec: Option<String>,
    sheet_place: Option<String>,
    sheet_medium: Option<i64>,
    sheet_side: Option<i64>,
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
                "document" => FileRole::Document {
                    kind: document_kind_of(row.document_kind.as_deref().ok_or_else(|| {
                        DbError::unreadable(
                            format!(
                                "the kind of document {} under {candidate_path}",
                                row.relative_path
                            ),
                            None::<String>,
                        )
                    })?)?,
                },
                "other" => FileRole::Other,
                "track_sheet" => {
                    let missing = |what: &str| {
                        DbError::unreadable(
                            format!(
                                "the {what} of track sheet {} under {candidate_path}",
                                row.relative_path
                            ),
                            None::<String>,
                        )
                    };
                    FileRole::TrackSheet {
                        sheet: row.sheet.ok_or_else(|| missing("parsed sheet"))?.sheet()?,
                        binding: sheet_binding_of(
                            &row.sheet_binding.ok_or_else(|| missing("binding"))?,
                            row.sheet_audio.into_iter().map(StoredSheetAudio::audio_file).collect(),
                            row.sheet_binding_codec,
                        )?,
                        place: SheetPlace::from_columns(
                            &format!(
                                "the place of track sheet {} under {candidate_path}",
                                row.relative_path
                            ),
                            &row.sheet_place.ok_or_else(|| missing("place"))?,
                            row.sheet_medium,
                            row.sheet_side,
                        )?,
                    }
                }
                other => return Err(DbError::unreadable("scan_candidate_file.role", other)),
            };
            Ok(CandidateFile {
                file: ScannedFile {
                    version: FileVersion {
                        path: row.absolute_path,
                        size: to_u64(row.size, "a file's size")?,
                        modified_at_ns: row.modified_at_ns,
                    },
                    relative_path: row.relative_path,
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
                return Err(DbError::unreadable(
                    format!(
                        "the audio track sheet {} under {candidate_path} is bound to, which is \
                         not the candidate's",
                        entry.file.relative_path
                    ),
                    &named.file_id,
                ));
            }
        }
    }
    Ok(())
}

fn source_audio_of(row: &FileRow) -> Result<Option<ScannedAudio>, DbError> {
    match (
        row.audio_content_type.as_deref(),
        row.audio_container.as_deref(),
        row.audio_duration_ms,
        row.audio_sample_rate_hz,
        row.audio_bits_per_sample,
        row.audio_bitrate_kbps,
        row.audio_channels,
    ) {
        (None, None, None, None, None, None, None) if row.role != "audio" => Ok(None),
        (
            Some(content_type),
            Some(container),
            Some(duration_ms),
            Some(sample_rate_hz),
            bits_per_sample,
            bitrate_kbps,
            Some(channels),
        ) if row.role == "audio" => {
            let content_type = ContentType::from_mime(content_type);
            if !content_type.is_audio() {
                return Err(DbError::unreadable(
                    format!("the content type of candidate audio file {}", row.relative_path),
                    content_type.as_str(),
                ));
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
                container: AudioContainer::from_name(container),
                duration_ms: to_u64(duration_ms, "an audio file's duration")?,
            }))
        }
        columns => Err(DbError::unreadable(
            format!("the source-audio facts of {} file {}", row.role, row.relative_path),
            columns,
        )),
    }
}

/// One `scan_skipped_file` row as [`super::fragments::ScanItems`] holds it.
#[derive(serde::Deserialize)]
struct SkippedFileRow {
    relative_path: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    absolute_path: PathBuf,
    size: i64,
    modified_at_ns: i64,
    role: String,
    audio_damage: Option<String>,
    audio_bits: Option<i64>,
    document_kind: Option<String>,
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
                role: proposed_role_of(
                    &row.role,
                    row.audio_damage.as_deref(),
                    row.audio_bits,
                    row.document_kind.as_deref(),
                )?,
            })
        })
        .collect()
}

/// One `scan_candidate_part` row as [`super::fragments::ScanItems`] holds it.
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

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
