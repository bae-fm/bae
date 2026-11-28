//! A candidate's file-tag snapshot: the tags read from its audio files and the
//! cover embedded in them, stored against the scan generation and file edit
//! revision it was read at, replaced only while both still stand, and read
//! back with the stamp it was read under: a candidate's whole reading, every
//! reading under a root by the file version it was read from, and the
//! embedded cover a candidate selects with the file it came from.

use super::fragments::TagSnapshot;
use super::*;
use crate::file_tag_snapshot::{
    EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot, FileTags, StoredCover,
    StoredTagReading, TagReading,
};
use crate::folder_scanner::FileVersion;
use crate::tables::release_groupings::fragments::EnteredCandidate;
use crate::tables::release_groupings::CandidateEntry;
use bae_store::statement_optional;
use bae_util::content_type::ContentType;

coven::read!(
    writer TagSnapshotAt(key: crate::CandidateKey),
    r#"
    SELECT tags.*
    FROM @crate::tables::folder_scans::fragments::TagSnapshot(
        wanted: (SELECT 0 AS at, :key AS candidate_key, NULL AS found)) AS tags
"#
);

coven::read!(
    writer EntryTagSnapshotAt(key: crate::CandidateKey),
    r#"
    SELECT entry.*, tags.*
    FROM @crate::tables::release_groupings::fragments::EnteredCandidate(key: :key) AS entry
    LEFT JOIN @crate::tables::folder_scans::fragments::TagSnapshot(
        wanted: (SELECT 0 AS at, :key AS candidate_key, NULL AS found)) AS tags ON 1
"#
);

// Match a candidate still at one scan generation and file edit revision: the
// write changes its row to what it holds, and the count says whether one
// matched.
coven::write!(
    MatchScannedCandidate(
        watched_folder_path: Vec<u8>,
        path: &'p crate::CandidateKey,
        generation: i64,
        file_edit_revision: i64,
    ),
    r#"
    UPDATE scan_candidate SET generation = generation
    WHERE watched_folder_path = :watched_folder_path AND path = :path
        AND generation = :generation AND file_edit_revision = :file_edit_revision
"#
);

coven::write!(
    InsertTagSnapshot(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        scan_generation: i64,
        file_edit_revision: i64,
        embedded_cover_source_relative_path: Option<&'p str>,
        embedded_cover_content_type: Option<&'p str>,
        embedded_cover_data: Option<&'p [u8]>,
    ),
    r#"
    INSERT INTO scan_candidate_tag_snapshot
        (watched_folder_path, candidate_path, scan_generation, file_edit_revision,
         embedded_cover_source_relative_path, embedded_cover_content_type, embedded_cover_data)
    VALUES (:watched_folder_path, :candidate_path, :scan_generation, :file_edit_revision,
        :embedded_cover_source_relative_path, :embedded_cover_content_type,
        :embedded_cover_data)
"#
);

coven::write!(
    InsertFileTag(
        watched_folder_path: &'p [u8],
        candidate_path: &'p crate::CandidateKey,
        relative_path: &'p str,
        file_size: i64,
        modified_at_ns: i64,
        unparseable_reason: Option<&'p str>,
        title: Option<&'p str>,
        track_artists: Option<&'p str>,
        album_title: Option<&'p str>,
        album_artists: Option<&'p str>,
        compilation: Option<bool>,
        years: Option<&'p str>,
        track_number: Option<i64>,
        disc_number: Option<i64>,
    ),
    r#"
    INSERT INTO scan_candidate_file_tag
        (watched_folder_path, candidate_path, relative_path, file_size, modified_at_ns,
         unparseable_reason, title, track_artists, album_title, album_artists, compilation,
         years, track_number, disc_number)
    VALUES (:watched_folder_path, :candidate_path, :relative_path, :file_size, :modified_at_ns,
        :unparseable_reason, :title, :track_artists, :album_title, :album_artists,
        :compilation, :years, :track_number, :disc_number)
"#
);

// The audio files the candidate at `candidate_path` holds, in play order.
coven::read!(
    writer CandidateAudioFiles(watched_folder_path: Vec<u8>, candidate_path: CandidateKey),
    r#"
    SELECT file.relative_path, file.size
    FROM scan_candidate_file AS file
    WHERE file.watched_folder_path = :watched_folder_path
      AND file.candidate_path = :candidate_path AND file.role = 'audio'
    ORDER BY file.position
"#
);

/// Replace a candidate's file-tag snapshot, inside the caller's transaction,
/// if its scan generation and file edit revision still match the snapshot's.
/// `false` means the candidate changed first and nothing was written.
pub(crate) fn replace_candidate_file_tag_snapshot_on(
    sql: &WriteContext<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> Result<bool, DbError> {
    let expected_generation = generation_column(snapshot.scan_generation)?;
    let expected_file_edit_revision =
        to_i64(snapshot.file_edit_revision, "a file-tag snapshot's file edit revision")?;
    let matched = sql.run(&MatchScannedCandidateParams {
        watched_folder_path: coven::path_bytes(watched_folder_path),
        path: candidate_path,
        generation: expected_generation,
        file_edit_revision: expected_file_edit_revision,
    })?;
    if matched == 0 {
        return Ok(false);
    }

    let audio_files: Vec<(String, i64)> = bae_store::statement_rows(
        sql,
        &CandidateAudioFilesParams {
            watched_folder_path: coven::path_bytes(watched_folder_path),
            candidate_path: candidate_path.clone(),
        },
    )?
    .into_iter()
    .map(|file| (file.relative_path, file.size))
    .collect();
    // The reading was taken from the audio the candidate held at this
    // generation and revision, which the match above confirmed it still
    // holds.
    assert!(
        audio_files.len() == snapshot.files.len()
            && audio_files.iter().zip(&snapshot.files).all(|((relative_path, size), fact)| {
                relative_path == &fact.observation.relative_path
                    && u64::try_from(*size).ok() == Some(fact.observation.size)
            }),
        "file-tag snapshot for {candidate_path} does not cover its current audio files"
    );
    assert!(
        snapshot.embedded_cover.as_ref().is_none_or(|cover| {
            snapshot
                .files
                .iter()
                .any(|fact| fact.observation.relative_path == cover.source_relative_path)
        }),
        "file-tag snapshot for {candidate_path} names an embedded cover outside its audio files"
    );

    replace_candidate_file_tag_snapshot(sql, watched_folder_path, candidate_path, snapshot)?;
    Ok(true)
}

/// Replace one candidate's stored file-tag reading, inside the caller's
/// transaction.
pub(crate) fn replace_candidate_file_tag_snapshot(
    sql: &WriteContext<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> Result<(), DbError> {
    let root = coven::path_bytes(watched_folder_path);
    sql.run(&super::write::DeleteEntryTagSnapshotParams {
        watched_folder_path: &root,
        key: candidate_path,
    })?;
    let (cover_source, cover_content_type, cover_data) = match &snapshot.embedded_cover {
        Some(cover) => (
            Some(cover.source_relative_path.as_str()),
            Some(cover.content_type.as_str()),
            Some(cover.data.as_slice()),
        ),
        None => (None, None, None),
    };
    sql.run(&InsertTagSnapshotParams {
        watched_folder_path: &root,
        candidate_path,
        scan_generation: to_i64(snapshot.scan_generation, "a file-tag snapshot's scan generation")?,
        file_edit_revision: to_i64(
            snapshot.file_edit_revision,
            "a file-tag snapshot's file edit revision",
        )?,
        embedded_cover_source_relative_path: cover_source,
        embedded_cover_content_type: cover_content_type,
        embedded_cover_data: cover_data,
    })?;
    for fact in &snapshot.files {
        let (unparseable_reason, stated) = match &fact.tags {
            TagReading::Stated(tags) => (None, Some(StatedTagColumns::of(tags)?)),
            TagReading::Unparseable(reason) => (Some(reason.as_str()), None),
        };
        let stated = stated.as_ref();
        sql.run(&InsertFileTagParams {
            watched_folder_path: &root,
            candidate_path,
            relative_path: &fact.observation.relative_path,
            file_size: to_i64(fact.observation.size, "a file-tag observation's size")?,
            modified_at_ns: fact.observation.modified_at_ns,
            unparseable_reason,
            title: stated.and_then(|tags| tags.title),
            track_artists: stated.map(|tags| tags.track_artists.as_str()),
            album_title: stated.and_then(|tags| tags.album_title),
            album_artists: stated.map(|tags| tags.album_artists.as_str()),
            compilation: stated.map(|tags| tags.compilation),
            years: stated.map(|tags| tags.years.as_str()),
            track_number: stated.and_then(|tags| tags.track_number),
            disc_number: stated.and_then(|tags| tags.disc_number),
        })?;
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
    /// The bytes of the embedded cover the candidate's snapshot stored from
    /// `source_file_id`, when it stored one from that file.
    pub(crate) async fn load_embedded_cover_bytes(
        &self,
        candidate_path: &CandidateKey,
        source_file_id: &str,
    ) -> Result<Option<Vec<u8>>, DbError> {
        let candidate_path = candidate_path.clone();
        let source_file_id = source_file_id.to_string();
        self.store
            .read(move |sql| stored_embedded_cover_bytes(&sql, &candidate_path, &source_file_id))
            .await
    }

    /// The candidate's entry and whatever file-tag snapshot is stored for
    /// it, even an outdated one, so a caller can tell a candidate never read
    /// from one whose reading is out of date.
    pub(crate) async fn load_candidate_file_tag_snapshot(
        &self,
        candidate_path: &CandidateKey,
    ) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
        let candidate_path = candidate_path.clone();
        self.store.read(move |sql| load_candidate_file_tag_snapshot(&sql, &candidate_path)).await
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

/// The candidate's entry and any snapshot stored for it, read as one
/// statement. The stored snapshot is returned even when its stamp is older
/// than the entry's, so the caller can distinguish an absent reading from one
/// invalidated by a newer scan or file decision. A blocked candidate still
/// displays its stored artwork and metadata: operations enforce
/// actionability before reading or changing source files.
pub(crate) fn load_candidate_file_tag_snapshot(
    sql: &impl bae_store::WriterRead,
    candidate_path: &CandidateKey,
) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
    let request = EntryTagSnapshotAtParams { key: candidate_path.clone() };
    let Some(EntryTagSnapshotAtRow { entry, tags }) = statement_optional(sql, &request)? else {
        return Ok(None);
    };
    let EnteredCandidate { scan, grouping, standing, imported } = entry;
    let Some(entry) = CandidateEntry::of(scan, grouping, standing, imported)? else {
        return Ok(None);
    };
    let snapshot = tags
        .map(|tags| StoredSnapshot::of(candidate_path, tags)?.snapshot(candidate_path))
        .transpose()?;
    Ok(Some(DbCandidateFileTagSnapshot { entry, snapshot }))
}

/// The file-tag reading stored for one scanned candidate, whatever the scan
/// row around it says. The candidate's own shape is the caller's to check.
pub(crate) fn load_file_tag_snapshot(
    sql: &impl bae_store::WriterRead,
    candidate_path: &CandidateKey,
) -> Result<Option<FileTagSnapshot>, DbError> {
    let request = TagSnapshotAtParams { key: candidate_path.clone() };
    statement_optional(sql, &request)?
        .map(|row| StoredSnapshot::of(candidate_path, row.tags)?.snapshot(candidate_path))
        .transpose()
}

/// Each file's fact in `json`, [`SnapshotFileTags`]' array of the snapshot
/// stored under `candidate_path`, with the path the file was read at.
///
/// [`SnapshotFileTags`]: super::fragments::SnapshotFileTags
fn file_tags_of(
    candidate_path: &CandidateKey,
    json: &str,
) -> Result<Vec<(PathBuf, FileTagFact)>, DbError> {
    let files: Vec<StoredFileTags> = serde_json::from_str(json).map_err(|error| {
        DbError::unreadable_json(
            format!("the file tags stored under {candidate_path}"),
            json,
            error,
        )
    })?;
    files.into_iter().map(|stored| Ok((stored.absolute_path.clone(), stored.fact()?))).collect()
}

/// What the tags of each track in `json`, [`file_tags_of`]'s array, state, or
/// why they do not parse, by the version of the track it was read from.
pub(super) fn track_readings(
    candidate_path: &CandidateKey,
    json: &str,
) -> Result<Vec<(FileVersion, TagReading)>, DbError> {
    Ok(file_tags_of(candidate_path, json)?
        .into_iter()
        .map(|(path, fact)| {
            let version = FileVersion {
                path,
                size: fact.observation.size,
                modified_at_ns: fact.observation.modified_at_ns,
            };
            (version, fact.tags)
        })
        .collect())
}

/// A stored file-tag reading, with its picture's bytes where the read took
/// them.
pub(super) struct StoredSnapshot {
    scan_generation: u64,
    file_edit_revision: u64,
    files: Vec<FileTagFact>,
    cover: Option<StoredCoverSource>,
}

/// The file a stored picture came from, what type it is, and its bytes where
/// the read took them.
struct StoredCoverSource {
    relative_path: String,
    content_type: ContentType,
    data: Option<Vec<u8>>,
}

impl StoredSnapshot {
    pub(super) fn of(candidate_path: &CandidateKey, row: TagSnapshot) -> Result<Self, DbError> {
        let cover =
            match (row.cover_source_relative_path, row.cover_content_type, row.has_cover_data) {
                (None, None, false) => None,
                (Some(relative_path), Some(content_type), true) => Some(StoredCoverSource {
                    relative_path,
                    content_type: ContentType::from_mime(&content_type),
                    data: row.cover_data,
                }),
                columns => {
                    return Err(DbError::unreadable(
                        format!("the embedded cover of {candidate_path}'s tag snapshot"),
                        (columns.0, columns.1, columns.2),
                    ))
                }
            };
        let files = file_tags_of(candidate_path, &row.file_tags.tag_files)?
            .into_iter()
            .map(|(_, fact)| fact)
            .collect();
        Ok(Self {
            scan_generation: to_u64(row.scan_generation, "a file-tag snapshot's scan generation")?,
            file_edit_revision: to_u64(
                row.tag_file_edit_revision,
                "a file-tag snapshot's file edit revision",
            )?,
            files,
            cover,
        })
    }

    /// The reading as it seeds a pass, from a read that took the picture's
    /// bytes only where the file it came from is unchanged: a picture with no
    /// bytes is of a file the pass reads from disk.
    pub(super) fn reading(self) -> StoredTagReading {
        let cover = match self.cover {
            None => StoredCover::Absent,
            Some(StoredCoverSource { relative_path, content_type, data: Some(data) }) => {
                StoredCover::Picture(EmbeddedCoverFact {
                    source_relative_path: relative_path,
                    content_type,
                    data,
                })
            }
            Some(StoredCoverSource { relative_path, data: None, .. }) => {
                StoredCover::Replaced { source_relative_path: relative_path }
            }
        };
        StoredTagReading { files: self.files, cover }
    }

    /// The reading, from a read that took its picture's bytes.
    fn snapshot(self, candidate_path: &CandidateKey) -> Result<FileTagSnapshot, DbError> {
        let embedded_cover = self
            .cover
            .map(|source| {
                let data = source.data.ok_or_else(|| {
                    DbError::unreadable(
                        format!("the embedded cover of {candidate_path}'s tag snapshot"),
                        source.relative_path.clone(),
                    )
                })?;
                Ok::<_, DbError>(EmbeddedCoverFact {
                    source_relative_path: source.relative_path,
                    content_type: source.content_type,
                    data,
                })
            })
            .transpose()?;
        Ok(FileTagSnapshot {
            scan_generation: self.scan_generation,
            file_edit_revision: self.file_edit_revision,
            files: self.files,
            embedded_cover,
        })
    }
}

coven::read!(
    writer StoredEmbeddedCover(candidate_path: CandidateKey, source_file_id: String),
    r#"
    SELECT snapshot.embedded_cover_data
    FROM scan_candidate_tag_snapshot AS snapshot
    WHERE snapshot.candidate_path = :candidate_path
      AND snapshot.embedded_cover_source_relative_path = :source_file_id
"#
);

/// The bytes of the embedded cover the candidate's tag snapshot stored from
/// `source_file_id`, or `None` when it stored none from that file.
pub(crate) fn stored_embedded_cover_bytes(
    sql: &impl bae_store::WriterRead,
    candidate_path: &CandidateKey,
    source_file_id: &str,
) -> Result<Option<Vec<u8>>, DbError> {
    let stored = StoredEmbeddedCoverParams {
        candidate_path: candidate_path.clone(),
        source_file_id: source_file_id.to_owned(),
    };
    Ok(bae_store::statement_optional(sql, &stored)?.and_then(|cover| cover.embedded_cover_data))
}

/// One `scan_candidate_file_tag` row, with the path of its file, as
/// [`SnapshotFileTags`] holds it.
///
/// [`SnapshotFileTags`]: super::fragments::SnapshotFileTags
#[derive(serde::Deserialize)]
struct StoredFileTags {
    #[serde(deserialize_with = "path_column::from_hex")]
    absolute_path: PathBuf,
    relative_path: String,
    #[serde(rename = "file_size")]
    size: i64,
    modified_at_ns: i64,
    unparseable_reason: Option<String>,
    title: Option<String>,
    track_artists: Option<String>,
    album_title: Option<String>,
    album_artists: Option<String>,
    #[serde(deserialize_with = "flag_of_integer")]
    compilation: Option<bool>,
    years: Option<String>,
    track_number: Option<i64>,
    disc_number: Option<i64>,
}

/// A flag a JSON document holds as the integer its column stores.
fn flag_of_integer<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    match <Option<i64> as serde::Deserialize>::deserialize(deserializer)? {
        None => Ok(None),
        Some(0) => Ok(Some(false)),
        Some(1) => Ok(Some(true)),
        Some(other) => Err(serde::de::Error::custom(format!("{other} is not a flag"))),
    }
}

impl StoredFileTags {
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

/// One candidate's entry and whatever file-tag snapshot is stored beneath
/// it. The stored snapshot may carry an older stamp: callers compare it with
/// the entry's before reuse, while replacement uses the entry's pair as a
/// compare-and-set expectation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DbCandidateFileTagSnapshot {
    pub entry: CandidateEntry,
    pub snapshot: Option<crate::file_tag_snapshot::FileTagSnapshot>,
}

#[cfg(test)]
#[path = "tag_snapshot_tests.rs"]
pub(crate) mod tests;
