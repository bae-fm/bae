//! What the rows the scans stored for one watched root say was read of each
//! file, for the next pass over the root to take rather than read again (see
//! [`FileReads`]).

use super::read::{load_items, RowSources, StoredFileTags, FILE_TAG_COLUMNS};
use super::*;
use crate::import::file_tag_snapshot::{FileTagReader, FileTagSnapshot, FileTags};
use crate::import::folder_scanner::{Coverage, FileReads, FileVersion};
use std::sync::Arc;

impl Database {
    /// What the scans of `watched_folder_path` stored about each file they
    /// read — its entries, its folders' sidecars, and the tags read from its
    /// releases' tracks — for a pass that reads any other track's tags
    /// through `tags`. A store that says two things about one file fails the
    /// read.
    pub(crate) async fn load_file_reads(
        &self,
        watched_folder_path: &str,
        tags: Arc<dyn FileTagReader>,
    ) -> Result<FileReads, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            let items = load_items(&sql, &watched_folder_path, RowSources::Scanned)?;
            let sidecars = load_sidecars(&sql, &watched_folder_path)?;
            let stored_tags = load_root_file_tags(&sql, &watched_folder_path)?;
            Ok((items, sidecars, stored_tags))
        })
        .process(move |(items, sidecars, stored_tags)| {
            let entries: Vec<ScanItem> = items()?
                .into_iter()
                .map(|stored| stored.item)
                .chain(sidecars.into_iter().map(ScanItem::Sidecar))
                .collect();
            FileReads::stored(&entries, &stored_tags, tags)
                .map_err(|error| DbError::Message(error.to_string()))
        })
        .await
    }

    /// The tag reading stored for the release at `candidate_path` under
    /// `watched_folder_path`, when the release stored there reads its files
    /// from `file_root`: each of its facts is then of the file at the same
    /// path below that folder.
    pub(crate) async fn load_stored_file_tag_reading(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        file_root: &Path,
    ) -> Result<Option<FileTagSnapshot>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let file_root = file_root.to_string_lossy().into_owned();
        self.read(move |sql| {
            let stored_root: Option<String> = sql
                .query_row(
                    "SELECT file_root FROM scan_candidate \
                     WHERE watched_folder_path = ? AND path = ?",
                    params![watched_folder_path, candidate_path],
                    |row| row.get(0),
                )
                .optional()?;
            if stored_root.as_deref() != Some(file_root.as_str()) {
                return Ok(None);
            }
            read::load_file_tag_snapshot(&sql, &watched_folder_path, &candidate_path)
        })
        .await
    }

    /// Whether a settled entry the scans stored under `watched_folder_path`
    /// reads any of the files `coverage` reads.
    pub(crate) async fn settled_entry_reads(
        &self,
        watched_folder_path: &str,
        coverage: &Coverage,
    ) -> Result<bool, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let coverage = coverage.clone();
        self.read(move |sql| {
            Ok(!read::entries_reading(
                &sql,
                &watched_folder_path,
                &coverage,
                read::Readers::Settled,
            )?
            .is_empty())
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
