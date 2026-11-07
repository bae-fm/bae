//! A candidate's file-tag snapshot: the tags read from its audio files and the
//! cover embedded in them, stored against the scan generation and file edit
//! revision it was read at, replaced only while both still stand, and read
//! back with the stamp it was read under: a candidate's whole reading, every
//! reading under a root by the file version it was read from, and the
//! embedded cover a candidate selects with the file it came from.

use super::*;
use crate::file_tag_snapshot::{
    EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot, FileTags, TagReading,
};
use crate::folder_scanner::FileVersion;
use bae_util::content_type::ContentType;

/// Replace a candidate's file-tag snapshot, inside the caller's transaction,
/// if its scan generation and file edit revision still match the snapshot's.
/// `false` means the candidate changed first and nothing was written.
pub(crate) fn replace_candidate_file_tag_snapshot_on(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> Result<bool, DbError> {
    let expected_generation = generation_column(snapshot.scan_generation)?;
    let expected_file_edit_revision =
        to_i64(snapshot.file_edit_revision, "a file-tag snapshot's file edit revision")?;
    let matched = sql.execute(
        "UPDATE scan_candidate SET generation = generation \
         WHERE watched_folder_path = ? AND path = ? \
           AND generation = ? AND file_edit_revision = ?",
        params![
            coven::path_bytes(watched_folder_path),
            candidate_path,
            expected_generation,
            expected_file_edit_revision
        ],
    )?;
    if matched == 0 {
        return Ok(false);
    }

    let audio_files: Vec<(String, i64)> = sql.query(
        "SELECT relative_path, size FROM scan_candidate_file \
         WHERE watched_folder_path = ? AND candidate_path = ? AND role = 'audio' \
         ORDER BY position",
        params![coven::path_bytes(watched_folder_path), candidate_path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if audio_files.len() != snapshot.files.len()
        || audio_files.iter().zip(&snapshot.files).any(|((relative_path, size), fact)| {
            relative_path != &fact.observation.relative_path
                || u64::try_from(*size).ok() != Some(fact.observation.size)
        })
    {
        return Err(DbError::Message(format!(
            "file-tag snapshot for {candidate_path} does not cover its current audio files"
        )));
    }
    if snapshot.embedded_cover.as_ref().is_some_and(|cover| {
        !snapshot
            .files
            .iter()
            .any(|fact| fact.observation.relative_path == cover.source_relative_path)
    }) {
        return Err(DbError::Message(format!(
            "file-tag snapshot for {candidate_path} names an embedded cover outside its audio files"
        )));
    }

    replace_candidate_file_tag_snapshot(sql, watched_folder_path, candidate_path, snapshot)?;
    Ok(true)
}

/// Replace one candidate's stored file-tag reading, inside the caller's
/// transaction.
pub(crate) fn replace_candidate_file_tag_snapshot(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM scan_candidate_tag_snapshot \
         WHERE watched_folder_path = ? AND candidate_path = ?",
        params![coven::path_bytes(watched_folder_path), candidate_path],
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
            coven::path_bytes(watched_folder_path),
            candidate_path,
            to_i64(snapshot.scan_generation, "a file-tag snapshot's scan generation")?,
            to_i64(snapshot.file_edit_revision, "a file-tag snapshot's file edit revision")?,
            cover_source,
            cover_content_type,
            cover_data,
        ],
    )?;
    for fact in &snapshot.files {
        let (unparseable_reason, stated) = match &fact.tags {
            TagReading::Stated(tags) => (None, Some(StatedTagColumns::of(tags)?)),
            TagReading::Unparseable(reason) => (Some(reason.as_str()), None),
        };
        let stated = stated.as_ref();
        sql.execute(
            "INSERT INTO scan_candidate_file_tag \
                 (watched_folder_path, candidate_path, relative_path, file_size, \
                  modified_at_ns, unparseable_reason, title, track_artists, album_title, \
                  album_artists, compilation, years, track_number, disc_number) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                coven::path_bytes(watched_folder_path),
                candidate_path,
                fact.observation.relative_path,
                to_i64(fact.observation.size, "a file-tag observation's size")?,
                fact.observation.modified_at_ns,
                unparseable_reason,
                stated.and_then(|tags| tags.title),
                stated.map(|tags| tags.track_artists.as_str()),
                stated.and_then(|tags| tags.album_title),
                stated.map(|tags| tags.album_artists.as_str()),
                stated.map(|tags| tags.compilation),
                stated.map(|tags| tags.years.as_str()),
                stated.and_then(|tags| tags.track_number),
                stated.and_then(|tags| tags.disc_number),
            ],
        )?;
    }
    Ok(())
}

/// The columns of what one file's tags state, as a row holds them.
struct StatedTagColumns<'a> {
    title: Option<&'a str>,
    track_artists: String,
    album_title: Option<&'a str>,
    album_artists: String,
    compilation: bool,
    years: String,
    track_number: Option<i64>,
    disc_number: Option<i64>,
}

impl<'a> StatedTagColumns<'a> {
    fn of(tags: &'a FileTags) -> Result<Self, DbError> {
        Ok(Self {
            title: tags.title.as_deref(),
            track_artists: tag_values_json(&tags.track_artists)?,
            album_title: tags.album_title.as_deref(),
            album_artists: tag_values_json(&tags.album_artists)?,
            compilation: tags.compilation,
            years: tag_values_json(&tags.years)?,
            track_number: tags.numbers.track.map(i64::from),
            disc_number: tags.numbers.disc.map(i64::from),
        })
    }
}

/// A multi-valued tag's values as their JSON array column holds them.
fn tag_values_json<T: serde::Serialize>(values: &[T]) -> Result<String, DbError> {
    serde_json::to_string(values).map_err(|error| DbError::encode("a file tag's values", error))
}

impl ImportTables {
    /// The tag reading stored for the release at `candidate_path` under
    /// `watched_folder_path`, when the release stored there reads its files
    /// from `file_root`: each of its facts is then of the file at the same
    /// path below that folder.
    pub(crate) async fn load_stored_file_tag_reading(
        &self,
        watched_folder_path: &Path,
        candidate_path: &CandidateKey,
        file_root: &Path,
    ) -> Result<Option<FileTagSnapshot>, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let candidate_path = candidate_path.clone();
        let file_root = coven::path_bytes(file_root);
        self.store
            .read(move |sql| {
                let stored_root: Option<Vec<u8>> = sql
                    .query_row(
                        "SELECT file_root FROM scan_candidate \
                     WHERE watched_folder_path = ? AND path = ?",
                        params![coven::path_bytes(&watched_folder_path), candidate_path],
                        |row| row.get(0),
                    )
                    .optional()?;
                if stored_root.as_ref() != Some(&file_root) {
                    return Ok(None);
                }
                super::tag_snapshot::load_file_tag_snapshot(
                    &sql,
                    &watched_folder_path,
                    &candidate_path,
                )
            })
            .await
    }

    /// The candidate's current scan generation and whatever file-tag snapshot
    /// is stored for it, even an outdated one, so a caller can tell a
    /// candidate never read from one whose reading is out of date.
    pub(crate) async fn load_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &Path,
        candidate_path: &CandidateKey,
    ) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let candidate_path = candidate_path.clone();
        self.store
            .read(move |sql| {
                load_candidate_file_tag_snapshot(&sql, &watched_folder_path, &candidate_path)
            })
            .await
    }

    /// [`replace_candidate_file_tag_snapshot_on`] in a transaction of its own.
    #[cfg(test)]
    pub(crate) async fn replace_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &Path,
        candidate_path: &CandidateKey,
        snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
    ) -> Result<bool, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let candidate_path = candidate_path.clone();
        let snapshot = snapshot.clone();
        self.store
            .call(move |sql| {
                replace_candidate_file_tag_snapshot_on(
                    sql,
                    &watched_folder_path,
                    &candidate_path,
                    &snapshot,
                )
            })
            .await
    }
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

/// The candidate's current scan stamp and any snapshot stored under it. The
/// stored snapshot is returned even when its stamp is older so the caller can
/// distinguish an absent reading from one invalidated by a newer scan or file
/// decision.
pub(crate) fn load_candidate_file_tag_snapshot(
    sql: &impl coven::SqlRead,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
    let Some(stored_candidate) =
        crate::tables::release_groupings::load_candidate_on(sql, candidate_path)?
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
    sql: &impl coven::SqlRead,
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
                    return Err(DbError::unreadable(
                        format!("the embedded cover of {candidate_path}'s tag snapshot"),
                        (columns.0, columns.1, columns.2.map(|data| data.len())),
                    ))
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
    sql: &impl coven::SqlRead,
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

/// Every tag reading stored for a scanned release under the root — what a
/// track's tags state, or why they do not parse — by the version of the track
/// it was read from.
pub(super) fn load_root_file_tags(
    sql: &impl coven::SqlRead,
    watched_folder_path: &Path,
) -> Result<Vec<(FileVersion, TagReading)>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT files.absolute_path, {FILE_TAG_COLUMNS} \
             FROM scan_candidate_file_tag AS tags \
             INNER JOIN scan_candidate_file AS files \
                 ON files.watched_folder_path = tags.watched_folder_path \
                AND files.candidate_path = tags.candidate_path \
                AND files.relative_path = tags.relative_path \
             INNER JOIN scan_candidate AS candidate \
                 ON candidate.watched_folder_path = tags.watched_folder_path \
                AND candidate.path = tags.candidate_path \
             WHERE tags.watched_folder_path = ? AND candidate.source_kind = 'folder'"
        ),
        [coven::path_bytes(watched_folder_path)],
        |row| Ok((row.get::<_, Vec<u8>>(0)?, StoredFileTags::of_row(row, 1)?)),
    )?;
    rows.into_iter()
        .map(|(absolute_path, stored)| {
            let fact = stored.fact()?;
            Ok((
                FileVersion {
                    path: path_column::decode(absolute_path)?,
                    size: fact.observation.size,
                    modified_at_ns: fact.observation.modified_at_ns,
                },
                fact.tags,
            ))
        })
        .collect()
}

/// The embedded cover the candidate selects from `source_file_id`: the bytes
/// its file-tag snapshot stored, and the version of the source file the
/// snapshot read them from.
pub(super) fn stored_embedded_cover(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    source_file_id: &str,
) -> Result<(FileVersion, Vec<u8>), DbError> {
    type CoverColumns =
        (Option<String>, Option<Vec<u8>>, Option<i64>, Option<i64>, Option<Vec<u8>>);
    let (stored_source, data, size, modified_at_ns, path): CoverColumns = sql
        .query_row(
            "SELECT snapshot.embedded_cover_source_relative_path, snapshot.embedded_cover_data, \
                    tags.file_size, tags.modified_at_ns, files.absolute_path \
             FROM scan_candidate_tag_snapshot AS snapshot \
             LEFT JOIN scan_candidate_file_tag AS tags \
               ON tags.watched_folder_path = snapshot.watched_folder_path \
              AND tags.candidate_path = snapshot.candidate_path \
              AND tags.relative_path = :source \
             LEFT JOIN scan_candidate_file AS files \
               ON files.watched_folder_path = snapshot.watched_folder_path \
              AND files.candidate_path = snapshot.candidate_path \
              AND files.relative_path = :source \
             WHERE snapshot.watched_folder_path = :root AND snapshot.candidate_path = :path",
            rusqlite::named_params! {
                ":root": coven::path_bytes(watched_folder_path),
                ":path": candidate_path,
                ":source": source_file_id,
            },
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| {
            DbError::unreadable(
                format!("the tag snapshot of {candidate_path}, whose cover choice is embedded"),
                None::<String>,
            )
        })?;
    let (Some(stored_source), Some(data)) = (stored_source, data) else {
        return Err(DbError::unreadable(
            format!("the embedded cover of {candidate_path}'s tag snapshot, which it chose"),
            None::<String>,
        ));
    };
    if stored_source != source_file_id {
        return Err(DbError::unreadable(
            format!("the source of {candidate_path}'s embedded cover and of its cover choice"),
            (&stored_source, source_file_id),
        ));
    }
    let (Some(size), Some(modified_at_ns)) = (size, modified_at_ns) else {
        return Err(DbError::unreadable(
            format!("the reading of {source_file_id}, the source of {candidate_path}'s cover"),
            (size, modified_at_ns),
        ));
    };
    let path = path.ok_or_else(|| {
        DbError::unreadable(
            format!("the file {candidate_path}'s embedded cover choice names"),
            source_file_id,
        )
    })?;
    Ok((
        FileVersion {
            path: path_column::decode(path)?,
            size: to_u64(size, "a file-tag reading's size")?,
            modified_at_ns,
        },
        data,
    ))
}

/// The `scan_candidate_file_tag` columns a query selects, in this order.
const FILE_TAG_COLUMNS: &str = "tags.relative_path, tags.file_size, tags.modified_at_ns, \
     tags.unparseable_reason, tags.title, tags.track_artists, tags.album_title, \
     tags.album_artists, tags.compilation, tags.years, tags.track_number, tags.disc_number";

/// One `scan_candidate_file_tag` row as SQLite returns it.
struct StoredFileTags {
    relative_path: String,
    size: i64,
    modified_at_ns: i64,
    unparseable_reason: Option<String>,
    title: Option<String>,
    track_artists: Option<String>,
    album_title: Option<String>,
    album_artists: Option<String>,
    compilation: Option<bool>,
    years: Option<String>,
    track_number: Option<i64>,
    disc_number: Option<i64>,
}

impl StoredFileTags {
    /// The row's [`FILE_TAG_COLUMNS`], selected from column `first` on.
    fn of_row(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            relative_path: row.get(first)?,
            size: row.get(first + 1)?,
            modified_at_ns: row.get(first + 2)?,
            unparseable_reason: row.get(first + 3)?,
            title: row.get(first + 4)?,
            track_artists: row.get(first + 5)?,
            album_title: row.get(first + 6)?,
            album_artists: row.get(first + 7)?,
            compilation: row.get(first + 8)?,
            years: row.get(first + 9)?,
            track_number: row.get(first + 10)?,
            disc_number: row.get(first + 11)?,
        })
    }

    fn fact(self) -> Result<FileTagFact, DbError> {
        let tags = match (
            self.unparseable_reason,
            self.track_artists,
            self.album_artists,
            self.compilation,
            self.years,
        ) {
            (None, Some(track_artists), Some(album_artists), Some(compilation), Some(years)) => {
                TagReading::Stated(FileTags {
                    title: self.title,
                    track_artists: tag_values(&track_artists, "track artists")?,
                    album_title: self.album_title,
                    album_artists: tag_values(&album_artists, "album artists")?,
                    compilation,
                    years: tag_values(&years, "years")?,
                    numbers: crate::track_numbers::TrackNumbers {
                        disc: self
                            .disc_number
                            .map(|value| to_u32(value, "a file-tag disc number"))
                            .transpose()?,
                        track: self
                            .track_number
                            .map(|value| to_u32(value, "a file-tag track number"))
                            .transpose()?,
                    },
                })
            }
            (Some(reason), None, None, None, None) => TagReading::Unparseable(reason),
            columns => {
                return Err(DbError::unreadable(
                    format!("the stored tags of {}", self.relative_path),
                    columns,
                ))
            }
        };
        Ok(FileTagFact {
            observation: FileObservation {
                relative_path: self.relative_path,
                size: to_u64(self.size, "a file-tag observation's size")?,
                modified_at_ns: self.modified_at_ns,
            },
            tags,
        })
    }
}

/// A multi-valued tag's values, read back from their JSON array column.
fn tag_values<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json).map_err(|error| {
        DbError::unreadable_json(format!("a file tag's stored {what}"), json, error)
    })
}

/// The current scan stamp for one candidate and whatever file-tag snapshot is
/// stored beneath it. The stored snapshot may carry an older stamp: callers
/// compare the two before reuse, while replacement uses the current pair as a
/// compare-and-set expectation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DbCandidateFileTagSnapshot {
    pub scan_generation: u64,
    pub candidate: crate::folder_scanner::FolderCandidate,
    pub snapshot: Option<crate::file_tag_snapshot::FileTagSnapshot>,
}

#[cfg(test)]
#[path = "tag_snapshot_tests.rs"]
pub(crate) mod tests;
