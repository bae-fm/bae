//! What the rows the scans stored for one watched root say was read of each
//! file, for the next pass over the root to take rather than read again (see
//! [`FileReads`]).

use super::read::{load_items, RowSources};
use super::tag_snapshot::load_root_file_tags;
use super::*;
use crate::file_tag_reader::FileTagReader;
use crate::folder_scanner::{Coverage, FileReads};
use std::sync::Arc;

impl ImportTables {
    /// What the scans of `watched_folder_path` stored about each file they
    /// read — its entries, its folders' sidecars, and the tags read from its
    /// releases' tracks — for a pass that reads any other track's tags
    /// through `tags`. A store that says two things about one file fails the
    /// read.
    pub(crate) async fn load_file_reads(
        &self,
        watched_folder_path: &Path,
        tags: Arc<dyn FileTagReader>,
    ) -> Result<FileReads, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        self.store
            .read(move |sql| {
                let items = load_items(&sql, &watched_folder_path, RowSources::Scanned)?;
                let sidecars = load_sidecars(&sql, &watched_folder_path)?;
                let stored_tags = load_root_file_tags(&sql, &watched_folder_path)?;
                Ok((items, sidecars, stored_tags))
            })
            .process(move |(items, sidecars, stored_tags)| {
                let entries: Vec<crate::folder_scanner::FolderEntry> = items()?
                    .into_iter()
                    .map(|stored| crate::folder_scanner::FolderEntry::Release(stored.item))
                    .chain(sidecars.into_iter().map(crate::folder_scanner::FolderEntry::Sidecar))
                    .collect();
                FileReads::stored(&entries, &stored_tags, tags)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }

    /// Whether a settled entry the scans stored under `watched_folder_path`
    /// reads any of the files `coverage` reads.
    pub(crate) async fn settled_entry_reads(
        &self,
        watched_folder_path: &Path,
        coverage: &Coverage,
    ) -> Result<bool, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let coverage = coverage.clone();
        self.store
            .read(move |sql| {
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
