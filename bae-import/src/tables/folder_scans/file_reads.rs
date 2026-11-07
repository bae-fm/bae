//! What the rows the scans stored for one watched root say was read of each
//! file, for the next pass over the root to take rather than read again (see
//! [`FileReads`]).

use super::read::{load_items, RowSources};
use super::tag_snapshot::load_root_file_tags;
use super::*;
use crate::file_tag_reader::FileTagReader;
use crate::file_tag_snapshot::FileTagSnapshot;
use crate::folder_scanner::{Coverage, FileReads, ScanItem};
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
    ) -> Result<FileReads, crate::ImportError> {
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
                Ok(FileReads::stored(&entries, &stored_tags, tags)?)
            })
            .await
    }

    /// What the store holds that seeds each of `items`, in their order, all
    /// in one read.
    pub(crate) async fn load_scan_batch_readings(
        &self,
        items: &[ScanItem],
    ) -> Result<Vec<ScanItemReadings>, DbError> {
        let wanted: Vec<_> = items.iter().map(ReadingsWanted::of).collect();
        self.store.read(move |sql| wanted.iter().map(|wanted| wanted.load(&sql)).collect()).await
    }
}

/// What the store holds that seeds one scan item.
#[derive(Default)]
pub(crate) struct ScanItemReadings {
    /// A tentative item whose files a settled entry already reads.
    pub(crate) settled: bool,
    /// The tag reading stored for the item's candidate, when it reads the
    /// same files. None for an item that is settled or has no files.
    pub(crate) stored: Option<FileTagSnapshot>,
}

/// What [`ImportTables::load_scan_batch_readings`] asks of the store for one
/// item.
struct ReadingsWanted {
    /// The candidate the item is, with its coverage; none for an item with no
    /// files to read tags from.
    candidate: Option<(PathBuf, CandidateKey, PathBuf, Coverage)>,
    /// Whether the item is tentative, so a settled entry may already read it.
    tentative: bool,
}

impl ReadingsWanted {
    fn of(item: &ScanItem) -> Self {
        let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = item else {
            return Self { candidate: None, tentative: false };
        };
        Self {
            candidate: Some((
                candidate.watched_folder_path.clone(),
                candidate.key(),
                candidate.file_root.clone(),
                item.coverage(),
            )),
            tentative: matches!(item, ScanItem::Discovered(_)),
        }
    }

    fn load(&self, sql: &SqlReadContext<'_>) -> Result<ScanItemReadings, DbError> {
        let Some((watched_folder_path, key, file_root, coverage)) = &self.candidate else {
            return Ok(ScanItemReadings::default());
        };
        if self.tentative
            && !read::entries_reading(sql, watched_folder_path, coverage, read::Readers::Settled)?
                .is_empty()
        {
            return Ok(ScanItemReadings { settled: true, stored: None });
        }
        Ok(ScanItemReadings {
            settled: false,
            stored: super::tag_snapshot::load_stored_file_tag_reading_on(
                sql,
                watched_folder_path,
                key,
                file_root,
            )?,
        })
    }
}

#[cfg(test)]
#[path = "file_reads_tests.rs"]
mod tests;
