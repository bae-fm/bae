//! What the rows the scans stored for one watched root say was read of each
//! file, for the next pass over the root to take rather than read again (see
//! [`FileReads`]).

use super::read::{load_items, RowSources, StoredScanItem};
use super::tag_snapshot::load_root_file_tags;
use super::*;
use crate::file_tag_reader::FileTagReader;
use crate::file_tag_snapshot::FileTagSnapshot;
use crate::folder_scanner::{Coverage, FileReads, ScanItem};
use std::collections::{HashMap, HashSet};
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
    /// in one read. An item stored at a key in `moved` is read for its seed
    /// whatever its entry says: that entry changed after a read found it
    /// unchanged.
    pub(crate) async fn load_scan_batch_readings(
        &self,
        items: &[ScanItem],
        moved: &HashSet<CandidateKey>,
    ) -> Result<Vec<ScanItemReadings>, DbError> {
        let wanted: Vec<_> = items.iter().map(|item| ReadingsWanted::of(item, moved)).collect();
        self.store
            .read(move |sql| {
                let keys: Vec<(&Path, &CandidateKey)> = wanted
                    .iter()
                    .filter_map(|wanted| match &wanted.candidate {
                        Some(candidate) if candidate.may_be_unchanged => {
                            Some((candidate.watched_folder_path.as_path(), &candidate.key))
                        }
                        _ => None,
                    })
                    .collect();
                let stored = read::load_items_by_keys(&sql, &keys, RowSources::Scanned)?;
                wanted.iter().map(|wanted| wanted.load(&sql, &stored)).collect()
            })
            .await
    }
}

/// What the store holds that seeds one scan item.
pub(crate) enum ScanItemReadings {
    /// A tentative item whose files a settled entry already reads.
    Settled,
    /// The entry stored at the item's key is exactly the item: its write
    /// keeps that entry and takes no seed.
    Unchanged,
    /// The tag reading stored for the item's candidate, when it reads the
    /// same files: none for an item with no tracks or no such reading.
    Stored(Option<FileTagSnapshot>),
}

/// What [`ImportTables::load_scan_batch_readings`] asks of the store for one
/// item.
struct ReadingsWanted {
    /// The candidate the item is; none for an item with no files to read
    /// tags from.
    candidate: Option<CandidateWanted>,
}

/// One candidate [`ReadingsWanted`] asks after.
struct CandidateWanted {
    item: ScanItem,
    watched_folder_path: PathBuf,
    key: CandidateKey,
    file_root: PathBuf,
    coverage: Coverage,
    /// Whether the item is tentative, so a settled entry may already read it.
    tentative: bool,
    /// Whether the entry stored at its key may be the item: false once that
    /// entry changed after a read found it so.
    may_be_unchanged: bool,
}

impl ReadingsWanted {
    fn of(item: &ScanItem, moved: &HashSet<CandidateKey>) -> Self {
        let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = item else {
            return Self { candidate: None };
        };
        let key = candidate.key();
        Self {
            candidate: Some(CandidateWanted {
                item: item.clone(),
                may_be_unchanged: !moved.contains(&key),
                watched_folder_path: candidate.watched_folder_path.clone(),
                key,
                file_root: candidate.file_root.clone(),
                coverage: item.coverage(),
                tentative: matches!(item, ScanItem::Discovered(_)),
            }),
        }
    }

    /// The item's readings, `stored` holding the entries at the keys of the
    /// items that may be unchanged. The checks are the write's, in its
    /// order: a tentative item a settled entry reads stands down, an item
    /// its entry already is keeps that entry, and any other is seeded.
    fn load(
        &self,
        sql: &SqlReadContext<'_>,
        stored: &HashMap<CandidateKey, StoredScanItem>,
    ) -> Result<ScanItemReadings, DbError> {
        let Some(wanted) = &self.candidate else {
            return Ok(ScanItemReadings::Stored(None));
        };
        let watched_folder_path = &wanted.watched_folder_path;
        if wanted.tentative
            && !read::entries_reading(
                sql,
                watched_folder_path,
                &wanted.coverage,
                read::Readers::Settled,
            )?
            .is_empty()
        {
            return Ok(ScanItemReadings::Settled);
        }
        if stored.get(&wanted.key).is_some_and(|stored| stored.item == wanted.item) {
            return Ok(ScanItemReadings::Unchanged);
        }
        Ok(ScanItemReadings::Stored(super::tag_snapshot::load_stored_file_tag_reading_on(
            sql,
            watched_folder_path,
            &wanted.key,
            &wanted.file_root,
        )?))
    }
}

#[cfg(test)]
#[path = "file_reads_tests.rs"]
mod tests;
