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
use crate::album_detail::AudioFormat;
use crate::cue_flac::{CueIndex, CueSheet, CueTrack, CueTrackMode};
use crate::import::file_tag_snapshot::{
    EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot, FileTags,
};
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, Coverage, FileRole, FileVersion, FolderCandidate,
    InvalidCandidate, ReleasePart, ScanItem, ScannedAudio, ScannedFile, SheetAudioFile,
    SkippedFile, SkippedTags,
};
use crate::import::{CandidateKey, RelativeFolder};
use bae_util::content_type::ContentType;
use track_sheets::{StoredSheet, StoredSheetAudio, SHEET_AUDIO_JSON, SHEET_JSON};

mod track_sheets;

type FinishRead<T> = Box<dyn FnOnce() -> Result<T, DbError> + Send + 'static>;

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
    Entry {
        root: &'a Path,
        key: &'a CandidateKey,
    },
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

struct StoredFileTagSnapshotRow {
    scan_generation: i64,
    file_edit_revision: i64,
    embedded_cover: StoredEmbeddedCoverColumns,
}

struct StoredEmbeddedCoverColumns {
    source_relative_path: Option<String>,
    content_type: Option<String>,
    data: Option<Vec<u8>>,
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
    let root_generation = sql
        .query(
            "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
            [coven::path_bytes(watched_folder_path)],
            |row| row.get::<_, i64>(0),
        )?
        .into_iter()
        .next()
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
    load_item_by_key_rows(sql, entry_key, sources)?
        .map(|process| process())
        .transpose()
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
            EntryRows::Entry {
                root: &root,
                key: entry_key,
            },
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

/// The candidate's current scan stamp and any snapshot stored under it. The
/// stored snapshot is returned even when its stamp is older so the caller can
/// distinguish an absent reading from one invalidated by a newer scan or file
/// decision.
pub(crate) fn load_candidate_file_tag_snapshot(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
    let Some(stored_candidate) =
        super::super::release_groupings::load_candidate_on(sql, candidate_path)?
    else {
        return Ok(None);
    };
    // A blocked candidate still displays its stored artwork and metadata.
    // Operations enforce actionability before reading or changing source files.
    let candidate = stored_candidate.candidate;
    if candidate.watched_folder_path.as_os_str() != watched_folder_path.as_os_str() {
        return Err(DbError::Message(format!(
            "candidate {candidate_path} does not belong to {}",
            coven::path_text(watched_folder_path)
        )));
    }

    let snapshot = load_file_tag_snapshot(sql, watched_folder_path, candidate_path)?;

    Ok(Some(DbCandidateFileTagSnapshot {
        scan_generation: stored_candidate.generation,
        candidate,
        snapshot,
    }))
}

/// The file-tag reading stored for one scanned candidate, whatever the scan
/// row around it says. The candidate's own shape is the caller's to check.
pub(crate) fn load_file_tag_snapshot(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
) -> Result<Option<FileTagSnapshot>, DbError> {
    let stored: Option<StoredFileTagSnapshotRow> = sql
        .query_row(
            "SELECT scan_generation, file_edit_revision, \
                    embedded_cover_source_relative_path, embedded_cover_content_type, \
                    embedded_cover_data \
             FROM scan_candidate_tag_snapshot \
             WHERE watched_folder_path = ? AND candidate_path = ?",
            params![coven::path_bytes(watched_folder_path), candidate_path],
            |row| {
                Ok(StoredFileTagSnapshotRow {
                    scan_generation: row.get(0)?,
                    file_edit_revision: row.get(1)?,
                    embedded_cover: StoredEmbeddedCoverColumns {
                        source_relative_path: row.get(2)?,
                        content_type: row.get(3)?,
                        data: row.get(4)?,
                    },
                })
            },
        )
        .optional()?;
    stored
        .map(|stored| {
                let embedded_cover = match (
                    stored.embedded_cover.source_relative_path,
                    stored.embedded_cover.content_type,
                    stored.embedded_cover.data,
                ) {
                    (None, None, None) => None,
                    (Some(source_relative_path), Some(content_type), Some(data)) => {
                        Some(EmbeddedCoverFact {
                            source_relative_path,
                            content_type: ContentType::from_mime(&content_type),
                            data,
                        })
                    }
                    columns => {
                        return Err(DbError::Message(format!(
                            "candidate {candidate_path}'s embedded cover is only partly stored: {columns:?}"
                        )))
                    }
                };
                Ok(FileTagSnapshot {
                    scan_generation: to_u64(
                        stored.scan_generation,
                        "a file-tag snapshot's scan generation",
                    )?,
                    file_edit_revision: to_u64(
                        stored.file_edit_revision,
                        "a file-tag snapshot's file edit revision",
                    )?,
                    files: load_file_tag_facts(sql, watched_folder_path, candidate_path)?,
                    embedded_cover,
                })
            })
        .transpose()
}

fn load_file_tag_facts(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
) -> Result<Vec<FileTagFact>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT {FILE_TAG_COLUMNS} \
         FROM scan_candidate_file_tag AS tags \
         INNER JOIN scan_candidate_file AS files \
             ON files.watched_folder_path = tags.watched_folder_path \
            AND files.candidate_path = tags.candidate_path \
            AND files.relative_path = tags.relative_path \
         WHERE tags.watched_folder_path = ? AND tags.candidate_path = ? \
         ORDER BY files.position"
        ),
        params![coven::path_bytes(watched_folder_path), candidate_path],
        |row| StoredFileTags::of_row(row, 0),
    )?;
    rows.into_iter().map(StoredFileTags::fact).collect()
}

/// The `scan_candidate_file_tag` columns a query selects, in this order.
pub(super) const FILE_TAG_COLUMNS: &str =
    "tags.relative_path, tags.file_size, tags.modified_at_ns, \
     tags.title, tags.track_artists, tags.album_title, tags.album_artists, tags.compilation, \
     tags.years, tags.track_number, tags.disc_number, tags.isrcs, tags.labels, tags.copyrights, \
     tags.store";

/// One `scan_candidate_file_tag` row as SQLite returns it.
pub(super) struct StoredFileTags {
    relative_path: String,
    size: i64,
    modified_at_ns: i64,
    title: Option<String>,
    track_artists: String,
    album_title: Option<String>,
    album_artists: String,
    compilation: bool,
    years: String,
    track_number: Option<i64>,
    disc_number: Option<i64>,
    isrcs: String,
    labels: String,
    copyrights: String,
    store: Option<String>,
}

impl StoredFileTags {
    /// The row's [`FILE_TAG_COLUMNS`], selected from column `first` on.
    pub(super) fn of_row(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            relative_path: row.get(first)?,
            size: row.get(first + 1)?,
            modified_at_ns: row.get(first + 2)?,
            title: row.get(first + 3)?,
            track_artists: row.get(first + 4)?,
            album_title: row.get(first + 5)?,
            album_artists: row.get(first + 6)?,
            compilation: row.get(first + 7)?,
            years: row.get(first + 8)?,
            track_number: row.get(first + 9)?,
            disc_number: row.get(first + 10)?,
            isrcs: row.get(first + 11)?,
            labels: row.get(first + 12)?,
            copyrights: row.get(first + 13)?,
            store: row.get(first + 14)?,
        })
    }

    pub(super) fn fact(self) -> Result<FileTagFact, DbError> {
        Ok(FileTagFact {
            observation: FileObservation {
                relative_path: self.relative_path,
                size: to_u64(self.size, "a file-tag observation's size")?,
                modified_at_ns: self.modified_at_ns,
            },
            tags: FileTags {
                title: self.title,
                track_artists: tag_values(&self.track_artists, "track artists")?,
                album_title: self.album_title,
                album_artists: tag_values(&self.album_artists, "album artists")?,
                compilation: self.compilation,
                years: tag_values(&self.years, "years")?,
                numbers: crate::import::track_numbers::TrackNumbers {
                    disc: self
                        .disc_number
                        .map(|value| to_u32(value, "a file-tag disc number"))
                        .transpose()?,
                    track: self
                        .track_number
                        .map(|value| to_u32(value, "a file-tag track number"))
                        .transpose()?,
                },
                isrcs: tag_values(&self.isrcs, "ISRCs")?,
                labels: tag_values(&self.labels, "labels")?,
                copyrights: tag_values(&self.copyrights, "copyright lines")?,
                store: self
                    .store
                    .map(|key| {
                        crate::signals::StoreMarker::from_key(&key).ok_or_else(|| {
                            DbError::Message(format!(
                                "a file-tag store {key:?} is no store bae reads"
                            ))
                        })
                    })
                    .transpose()?,
            },
        })
    }
}

/// A multi-valued tag's values, read back from their JSON array column.
fn tag_values<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json).map_err(|error| {
        DbError::Message(format!("a file tag's stored {what} do not read: {error}"))
    })
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
    let below = coverage
        .whole_subtree
        .then(|| crate::import::relative_folder::paths_below(&coverage.folder));
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
                other => return Err(unreadable("kind", other)),
            };
            items.push(StoredScanItem {
                key: row.path,
                generation,
                item,
            });
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
        DbError::Message(format!(
            "the {what} stored under {candidate_path} do not read: {error}"
        ))
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
                            row.sheet_audio
                                .into_iter()
                                .map(StoredSheetAudio::audio_file)
                                .collect(),
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
            bits_per_sample
                .map(|value| to_u64(value, "an audio file's bit depth"))
                .transpose()?;
            bitrate_kbps
                .map(|value| to_u64(value, "an audio file's bitrate"))
                .transpose()?;
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
        .map(|part| ReleasePart {
            folder: part.folder,
            prefix: part.prefix,
        })
        .collect())
}
