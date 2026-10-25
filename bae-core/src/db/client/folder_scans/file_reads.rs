//! What the rows the scans stored for one watched root say was read of each
//! file, for the next pass over the root to take rather than read again (see
//! [`FileReads`]).

use super::read::{load_items, RowSources, StoredFileTags, FILE_TAG_COLUMNS};
use super::*;
use crate::import::file_tag_snapshot::{FileTagSnapshot, FileTags};
use crate::import::folder_scanner::{FileReads, FileVersion, ScannedFile};

impl Database {
    /// What the scans of `watched_folder_path` stored about each file they
    /// read: its entries, its folders' sidecars, and the tags read from its
    /// releases' tracks. A store that says two things about one file fails
    /// the read.
    pub(crate) async fn load_file_reads(
        &self,
        watched_folder_path: &str,
    ) -> Result<FileReads, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            let items = load_items(&sql, &watched_folder_path, RowSources::Scanned)?;
            let sidecars = sql
                .query(
                    "SELECT folder FROM scan_sidecar WHERE watched_folder_path = ? ORDER BY folder",
                    [&watched_folder_path],
                    |row| row.get::<_, String>(0),
                )?
                .into_iter()
                .map(|folder| {
                    load_sidecar(&sql, Path::new(&folder))?.ok_or_else(|| {
                        DbError::Message(format!(
                            "the sidecar of {folder} vanished between its two reads"
                        ))
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            let tags = load_root_file_tags(&sql, &watched_folder_path)?;
            Ok((items, sidecars, tags))
        })
        .process(|(items, sidecars, tags)| {
            let entries: Vec<ScanItem> = items()?
                .into_iter()
                .map(|stored| stored.item)
                .chain(sidecars.into_iter().map(ScanItem::Sidecar))
                .collect();
            FileReads::stored(&entries, &tags).map_err(|error| DbError::Message(error.to_string()))
        })
        .await
    }
}

impl Database {
    /// The tag reading stored for the release at `candidate_path` under
    /// `watched_folder_path`, when it was read from exactly `audio_files`: the
    /// same tracks at the same paths, in the same order, each at the version
    /// it has now.
    pub(crate) async fn load_file_tag_snapshot_read_from(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        audio_files: Vec<ScannedFile>,
    ) -> Result<Option<FileTagSnapshot>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        self.read(move |sql| {
            let Some(snapshot) =
                read::load_file_tag_snapshot(&sql, &watched_folder_path, &candidate_path)?
            else {
                return Ok(None);
            };
            let stored: Vec<(String, String)> = sql.query(
                "SELECT relative_path, absolute_path FROM scan_candidate_file \
                 WHERE watched_folder_path = ? AND candidate_path = ? AND role = 'audio' \
                 ORDER BY position",
                params![watched_folder_path, candidate_path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let same_tracks = stored.len() == audio_files.len()
                && stored.iter().zip(&audio_files).all(|((relative_path, path), file)| {
                    *relative_path == file.relative_path && Path::new(path) == file.path
                });
            Ok((same_tracks && snapshot.was_read_from(audio_files.iter())).then_some(snapshot))
        })
        .await
    }
}

/// Every tag reading stored for a scanned release under the root, by the
/// version of the track it was read from.
fn load_root_file_tags(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
) -> Result<Vec<(FileVersion, FileTags)>, DbError> {
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
        [watched_folder_path],
        |row| Ok((row.get::<_, String>(0)?, StoredFileTags::of_row(row, 1)?)),
    )?;
    rows.into_iter()
        .map(|(absolute_path, stored)| {
            let fact = stored.fact()?;
            Ok((
                FileVersion {
                    path: PathBuf::from(absolute_path),
                    size: fact.observation.size,
                    modified_at_ns: fact.observation.modified_at_ns,
                },
                fact.tags,
            ))
        })
        .collect()
}
