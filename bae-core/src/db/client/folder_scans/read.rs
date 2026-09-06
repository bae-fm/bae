//! Reading scan items back out of the rows [`super::write`] laid down.
//!
//! One query per table rather than one per candidate: a watched root holds a
//! few thousand file rows and a handful of boundaries, and the assembly below
//! groups them by the key they hang off.

use super::columns::*;
use super::write::StoredEntry;
use super::*;
use crate::album_detail::AudioFormat;
use crate::cue_flac::{CueIndex, CuePregap, CueSheet, CueTrack, CueTrackMode};
use crate::import::file_tag_snapshot::{
    EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot,
};
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, FolderReleaseDecisionKey,
    InvalidCandidate, ResolvedFolderReleaseBoundary, ScanItem, ScannedAudio, ScannedFile,
    SheetAudioFile,
};
use crate::util::content_type::ContentType;

/// One stored entry: the key it is addressed by, the scan generation that
/// wrote it, and the item itself.
pub(crate) struct StoredScanItem {
    pub(crate) key: String,
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
    watched_folder_path: &str,
) -> Result<Vec<StoredScanItem>, DbError> {
    let mut items = load_candidate_items(sql, watched_folder_path, None)?;
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
        "SELECT watched_folder_path FROM scan_candidate WHERE path = ? AND source_kind = 'folder'",
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
    Ok(None)
}

/// The candidate's current scan stamp and any snapshot stored under it. The
/// stored snapshot is returned even when its stamp is older so the caller can
/// distinguish an absent reading from one invalidated by a newer scan or file
/// decision.
pub(crate) fn load_candidate_file_tag_snapshot(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    candidate_path: &str,
) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
    let Some(stored_candidate) =
        super::super::import_combinations::load_candidate_on(sql, candidate_path)?
    else {
        return Ok(None);
    };
    // A blocked candidate still displays its stored artwork and metadata.
    // Operations enforce actionability before reading or changing source files.
    let candidate = stored_candidate.candidate;
    if candidate.watched_folder_path() != watched_folder_path {
        return Err(DbError::Message(format!(
            "candidate {candidate_path} does not belong to {watched_folder_path}"
        )));
    }

    let stored: Option<StoredFileTagSnapshotRow> = sql
        .query_row(
            "SELECT scan_generation, file_edit_revision, \
                    embedded_cover_source_relative_path, embedded_cover_content_type, \
                    embedded_cover_data \
             FROM scan_candidate_tag_snapshot \
             WHERE watched_folder_path = ? AND candidate_path = ?",
            params![watched_folder_path, candidate_path],
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
    let snapshot = stored
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
        .transpose()?;

    Ok(Some(DbCandidateFileTagSnapshot {
        scan_generation: stored_candidate.generation,
        candidate,
        snapshot,
    }))
}

fn load_file_tag_facts(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    candidate_path: &str,
) -> Result<Vec<FileTagFact>, DbError> {
    let rows = sql.query(
        "SELECT tags.relative_path, tags.file_size, tags.modified_at_ns, \
                tags.title, tags.track_artist, tags.album_title, tags.album_artist, \
                tags.year, tags.track_number, tags.disc_number \
         FROM scan_candidate_file_tag AS tags \
         INNER JOIN scan_candidate_file AS files \
             ON files.watched_folder_path = tags.watched_folder_path \
            AND files.candidate_path = tags.candidate_path \
            AND files.relative_path = tags.relative_path \
         WHERE tags.watched_folder_path = ? AND tags.candidate_path = ? \
         ORDER BY files.position",
        params![watched_folder_path, candidate_path],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<i64>>(9)?,
            ))
        },
    )?;
    rows.into_iter()
        .map(
            |(
                relative_path,
                size,
                modified_at_ns,
                title,
                track_artist,
                album_title,
                album_artist,
                year,
                track_number,
                disc_number,
            )| {
                Ok(FileTagFact {
                    observation: FileObservation {
                        relative_path,
                        size: to_u64(size, "a file-tag observation's size")?,
                        modified_at_ns,
                    },
                    title,
                    track_artist,
                    album_title,
                    album_artist,
                    year: year
                        .map(|value| {
                            u16::try_from(value).map_err(|_| {
                                DbError::Message(
                                    "a file-tag year is outside the range it counts over"
                                        .to_string(),
                                )
                            })
                        })
                        .transpose()?,
                    track_number: track_number
                        .map(|value| to_u32(value, "a file-tag track number"))
                        .transpose()?,
                    disc_number: disc_number
                        .map(|value| to_u32(value, "a file-tag disc number"))
                        .transpose()?,
                })
            },
        )
        .collect()
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
            "SELECT path, scope FROM scan_candidate WHERE watched_folder_path = ? AND source_kind = 'folder'",
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
                file_edit_revision, combine_ancestor_relative_path, \
                invalid_reason, invalid_reason_path \
         FROM scan_candidate \
         WHERE watched_folder_path = :root AND source_kind = 'folder' AND (:only IS NULL OR path = :only) \
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
                combine_ancestor_relative_path: row.get(8)?,
                invalid_reason: row.get(9)?,
                invalid_reason_path: row.get(10)?,
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
    modified_at_ns: i64,
    audio_content_type: Option<String>,
    audio_duration_ms: Option<i64>,
    audio_sample_rate_hz: Option<i64>,
    audio_bits_per_sample: Option<i64>,
    audio_bitrate_kbps: Option<i64>,
    audio_channels: Option<i64>,
    file_name: String,
    dir_prefix: Option<String>,
    proposed_audio: bool,
    role: String,
    sheet_binding: Option<String>,
    sheet_binding_codec: Option<String>,
    sheet_disc: Option<String>,
    sheet_disc_number: Option<i64>,
}

pub(crate) fn load_files(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<String, Vec<CandidateFile>>, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, relative_path, absolute_path, size, modified_at_ns, \
                audio_content_type, audio_duration_ms, audio_sample_rate_hz, \
                audio_bits_per_sample, audio_bitrate_kbps, audio_channels, file_name, dir_prefix, \
                proposed_audio, role, sheet_binding, sheet_binding_codec, sheet_disc, \
                sheet_disc_number \
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
                modified_at_ns: row.get(4)?,
                audio_content_type: row.get(5)?,
                audio_duration_ms: row.get(6)?,
                audio_sample_rate_hz: row.get(7)?,
                audio_bits_per_sample: row.get(8)?,
                audio_bitrate_kbps: row.get(9)?,
                audio_channels: row.get(10)?,
                file_name: row.get(11)?,
                dir_prefix: row.get(12)?,
                proposed_audio: row.get(13)?,
                role: row.get(14)?,
                sheet_binding: row.get(15)?,
                sheet_binding_codec: row.get(16)?,
                sheet_disc: row.get(17)?,
                sheet_disc_number: row.get(18)?,
            })
        },
    )?;
    let mut sheets = load_cue_sheets(sql, watched_folder_path, only)?;
    let mut sheet_audio_files = load_sheet_audio_files(sql, watched_folder_path, only)?;
    let mut files: HashMap<String, Vec<CandidateFile>> = HashMap::new();
    for row in rows {
        let source_audio = source_audio_of(&row)?;
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
                        sheet_audio_files
                            .remove(&SheetKey {
                                candidate_path: row.candidate_path.clone(),
                                sheet_relative_path: row.relative_path.clone(),
                            })
                            .unwrap_or_default(),
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
                    modified_at_ns: row.modified_at_ns,
                    dir_prefix: row.dir_prefix,
                    file_name: row.file_name,
                    source_audio,
                },
                role,
                proposed_audio: row.proposed_audio,
            });
    }
    for (candidate_path, files) in &files {
        check_sheet_bindings_name_audio(candidate_path, files)?;
    }
    Ok(files)
}

/// A binding is settled against the roles in force, so every file it names
/// has the audio role. The table can only say the file exists; this says it
/// is audio, so a candidate that violates it is refused here rather than
/// panicking whoever reads the bound sheets.
fn check_sheet_bindings_name_audio(
    candidate_path: &str,
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
        (None, None, None, None, None, None) if !row.proposed_audio => Ok(None),
        (
            Some(content_type),
            Some(duration_ms),
            Some(sample_rate_hz),
            bits_per_sample,
            bitrate_kbps,
            Some(channels),
        ) if row.proposed_audio => {
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

/// The audio each bound sheet describes, in the sheet's reference order.
fn load_sheet_audio_files(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
    only: Option<&str>,
) -> Result<HashMap<SheetKey, Vec<SheetAudioFile>>, DbError> {
    let rows = sql.query(
        "SELECT candidate_path, sheet_relative_path, file_reference, audio_relative_path \
         FROM scan_sheet_audio_file \
         WHERE watched_folder_path = :root AND (:only IS NULL OR candidate_path = :only) \
         ORDER BY candidate_path, sheet_relative_path, position",
        named_params! { ":root": watched_folder_path, ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let mut audio_files: HashMap<SheetKey, Vec<SheetAudioFile>> = HashMap::new();
    for (candidate_path, sheet_relative_path, file_reference, audio_relative_path) in rows {
        audio_files
            .entry(SheetKey {
                candidate_path,
                sheet_relative_path,
            })
            .or_default()
            .push(SheetAudioFile {
                file_reference,
                file_id: audio_relative_path,
            });
    }
    Ok(audio_files)
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
