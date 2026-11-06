//! The folder-scan tables: `folder_scan_roots`, `folder_scan_directory`, the
//! `scan_candidate` family and the `scan_sidecar` pair. A scan generation is
//! durable before traversal begins, each item deletes what it supersedes as it
//! is written, and successful completion prunes rows of other generations in
//! the transaction that marks the root complete.
//!
//! [`write`] stores candidate rows, [`read`] loads them back, [`sidecar`]
//! stores a folder's sidecar files, [`file_reads`] reads what they say was
//! read of each file, [`tag_snapshot`] stores and reads back what a
//! candidate's files' tags read, [`cover_choice`] checks a cover against what
//! the scan stored and reads the one a stored selection names, and
//! [`file_decisions`] settles
//! the rows a file decision reshapes. Their tables are `folder_scans.sql`,
//! which every device's library holds.

/// The folder scans' part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("folder_scans.sql")]);

pub(super) mod columns;
mod cover_choice;
mod dates;
mod file_decisions;
mod file_reads;
mod progress;
pub(super) mod read;
mod reading;
mod sidecar;
pub(super) mod tag_snapshot;
mod write;

use super::release_groupings::{store_folder_reading, validate_decision_key_ownership};
use super::*;
use crate::folder_scanner::ScanItem;
use crate::CandidateKey;
use bae_store::query::{QueryOne, QueryRows};
use std::path::{Path, PathBuf};

// `use super::*` also brings in the client's own `read` and `write`, hence `self::`.
pub(super) use self::cover_choice::{cover_refusal, stored_cover_choice};
pub(super) use self::read::{load_item_by_key, require_current_candidate, RowSources};
/// Named only by a test that commits a reading under a stamp it took.
#[cfg(test)]
pub(crate) use self::reading::FolderReadingStamp;
pub(crate) use self::reading::{FolderReadingCommit, FolderReadingWrite};
pub(super) use self::tag_snapshot::load_candidate_file_tag_snapshot;
pub(crate) use self::tag_snapshot::DbCandidateFileTagSnapshot;

pub(super) use self::dates::forget_discovery_at_or_below;
pub(super) use self::file_decisions::settle_candidate_files;
pub(super) use self::sidecar::load_sidecar;
use self::sidecar::load_sidecars;
pub(super) use self::write::{delete_entry, insert_candidate_files};

/// What finishing a scan generation removed: the entries it did not see, and
/// the releases groupings rebuilt without them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FinishedScan {
    pub pruned: Vec<CandidateKey>,
    pub regrouped: super::release_groupings::GroupingChanges,
}

/// What one scan item's write did. A re-read of an unchanged folder is all
/// `Unchanged`, so it announces nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanItemWrite {
    /// The stored row already said this; it only took this generation's
    /// stamp, so the completion prune keeps it.
    Unchanged,
    /// The row was written, replacing the entries at `superseded_keys`;
    /// `regrouped` is the grouping releases rebuilt from what changed.
    Stored {
        superseded_keys: Vec<CandidateKey>,
        regrouped: super::release_groupings::GroupingChanges,
        /// See `EntryWrite::Stored`.
        found: bool,
    },
}

impl ScanItemWrite {
    /// Whether the row now says something it did not say before.
    pub fn changed(&self) -> bool {
        matches!(self, Self::Stored { .. })
    }

    /// The stored entries this write replaced. Empty when it wrote nothing.
    pub fn superseded_keys(&self) -> &[CandidateKey] {
        match self {
            Self::Unchanged => &[],
            Self::Stored { superseded_keys, .. } => superseded_keys,
        }
    }

    /// The releases groupings rebuilt because of this write.
    pub fn regrouped(&self) -> Option<&super::release_groupings::GroupingChanges> {
        match self {
            Self::Unchanged => None,
            Self::Stored { regrouped, .. } => Some(regrouped),
        }
    }

    /// Whether this write stored a release that is found: new to the library,
    /// or listed anew by a person's decision about how its folder reads.
    pub fn found(&self) -> bool {
        matches!(self, Self::Stored { found: true, .. })
    }
}

/// The generation `watched_folder_path` was last scanned under, `None` for a
/// root never scanned.
pub(super) fn root_generation_on(
    sql: &impl QueryOne,
    watched_folder_path: &Path,
) -> Result<Option<i64>, DbError> {
    Ok(sql
        .query_row(
            "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
            [coven::path_bytes(watched_folder_path)],
            |row| row.get(0),
        )
        .optional()?)
}

/// Refuse to write under a generation the root has moved past. Runs inside the
/// write transaction, so a scan that lost the root after the caller's check
/// cannot write over its successor.
fn ensure_generation(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
) -> Result<(), DbError> {
    if root_generation_on(sql, watched_folder_path)? != Some(generation) {
        return Err(DbError::Message(format!(
            "folder scan generation {generation} is no longer {}'s: \
             a newer scan took the root between the check and the write",
            coven::path_text(watched_folder_path)
        )));
    }
    Ok(())
}

/// Whether `snapshot` was taken from the audio `item` holds. Only a
/// tentative or valid candidate holds audio.
fn item_was_read_for(
    item: &ScanItem,
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> bool {
    match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            snapshot.was_read_from(candidate.files.audio())
        }
        ScanItem::Invalid(_) => false,
    }
}

fn generation_column(generation: u64) -> Result<i64, DbError> {
    to_i64(generation, "folder scan generation")
}

impl ImportTables {
    /// The root's generation, for a caller that records a failure against
    /// whichever generation stands now.
    pub(crate) async fn current_folder_scan_generation(
        &self,
        watched_folder_path: &Path,
    ) -> Result<Option<u64>, DbError> {
        self.current_scan_generation(watched_folder_path)
            .await?
            .map(|generation| to_u64(generation, "folder_scan_roots.generation"))
            .transpose()
    }

    /// The root's generation on the read connection, so a scan that lost the
    /// root finds out without opening a write.
    async fn current_scan_generation(
        &self,
        watched_folder_path: &Path,
    ) -> Result<Option<i64>, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        self.store.read(move |sql| root_generation_on(&sql, &watched_folder_path)).await
    }

    /// Open a new scan generation for `watched_folder_path`, recording the
    /// volume the folder is on.
    pub async fn begin_folder_scan(
        &self,
        watched_folder_path: &Path,
        volume: crate::VolumeKind,
    ) -> Result<u64, DbError> {
        let watched_folder_path = coven::path_bytes(watched_folder_path);
        let volume = columns::volume_text(volume);
        self.store
            .call(move |sql| {
                let generation = next_folder_scan_generation(sql)?;
                sql.execute(
                    "INSERT INTO folder_scan_roots \
                     (watched_folder_path, generation, status, error, volume) \
                 VALUES (?, ?, 'scanning', NULL, ?) \
                 ON CONFLICT(watched_folder_path) DO UPDATE SET \
                     generation = excluded.generation, status = 'scanning', error = NULL, \
                     volume = excluded.volume",
                    params![watched_folder_path, generation, volume],
                )?;
                to_u64(generation, "a folder scan generation")
            })
            .await
    }

    /// Store scan results in one transaction, in the order given, each
    /// deleting the entries it supersedes: a crash loses all of them or none.
    /// Each write sees the ones before it. `None`, with nothing written, when
    /// `generation` is no longer the root's.
    pub(crate) async fn save_folder_scan_items(
        &self,
        watched_folder_path: &Path,
        generation: u64,
        items: Vec<FolderEntryToWrite>,
    ) -> Result<Option<Vec<(crate::folder_scanner::FolderEntry, ScanItemWrite)>>, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let generation = generation_column(generation)?;
        let observed_at =
            chrono::DateTime::<chrono::Utc>::from(self.store.now()).timestamp_millis();
        if self.current_scan_generation(&watched_folder_path).await? != Some(generation) {
            return Ok(None);
        }
        self.store
            .call(move |sql| {
                ensure_generation(sql, &watched_folder_path, generation)?;
                let mut writes = Vec::with_capacity(items.len());
                for to_write in items {
                    let write = write_scan_item(
                        sql,
                        &watched_folder_path,
                        generation,
                        &to_write,
                        observed_at,
                        Finding::FirstRead,
                    )?;
                    writes.push((to_write.into_entry(), write));
                }
                Ok(Some(writes))
            })
            .await
    }

    /// Store one release entry with no seed and no folder date, as the scan's
    /// batched write stores it.
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    pub async fn save_folder_scan_item(
        &self,
        watched_folder_path: &Path,
        generation: u64,
        item: &ScanItem,
    ) -> Result<Option<ScanItemWrite>, DbError> {
        self.save_one_folder_entry(
            watched_folder_path,
            generation,
            FolderEntryToWrite::Release(ScanItemToWrite {
                item: item.clone(),
                seed: ScanItemSeed::Tags(None),
                folder_date: None,
            }),
        )
        .await
    }

    /// Store one folder's sidecar files, as the scan's batched write stores
    /// them.
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    pub async fn save_folder_sidecar(
        &self,
        watched_folder_path: &Path,
        generation: u64,
        sidecar: &crate::folder_scanner::FolderSidecar,
    ) -> Result<Option<ScanItemWrite>, DbError> {
        self.save_one_folder_entry(
            watched_folder_path,
            generation,
            FolderEntryToWrite::Sidecar(sidecar.clone()),
        )
        .await
    }

    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    async fn save_one_folder_entry(
        &self,
        watched_folder_path: &Path,
        generation: u64,
        entry: FolderEntryToWrite,
    ) -> Result<Option<ScanItemWrite>, DbError> {
        let writes =
            self.save_folder_scan_items(watched_folder_path, generation, vec![entry]).await?;
        Ok(writes.map(|mut writes| writes.pop().expect("one entry stores one write").1))
    }

    /// Replace the directories recorded for `watched_folder_path` with those a
    /// completed walk read, and their mtimes. A walk that missed any mtime
    /// records none, and a root with nothing recorded is never taken to be
    /// unchanged.
    pub async fn record_folder_scan_directories(
        &self,
        watched_folder_path: &Path,
        directories: &[(PathBuf, i64)],
    ) -> Result<(), DbError> {
        let watched_folder_path = coven::path_bytes(watched_folder_path);
        let directories = directories.to_vec();
        self.store
            .call(move |sql| {
                sql.execute(
                    "DELETE FROM folder_scan_directory WHERE watched_folder_path = ?",
                    [&watched_folder_path],
                )?;
                for (path, modified_at) in &directories {
                    sql.execute(
                    "INSERT INTO folder_scan_directory (watched_folder_path, path, modified_at) \
                     VALUES (?, ?, ?)",
                    params![watched_folder_path, coven::path_bytes(path), modified_at],
                )?;
                }
                Ok(())
            })
            .await
    }

    /// The directories and mtimes recorded for this root; empty when none are.
    pub async fn load_folder_scan_directories(
        &self,
        watched_folder_path: &Path,
    ) -> Result<Vec<(PathBuf, i64)>, DbError> {
        let watched_folder_path = coven::path_bytes(watched_folder_path);
        self.store
            .read(move |sql| {
                Ok(sql.query(
                    "SELECT path, modified_at FROM folder_scan_directory \
                 WHERE watched_folder_path = ?",
                    [watched_folder_path],
                    |row| Ok((path_column::get(row, 0)?, row.get::<_, i64>(1)?)),
                )?)
            })
            .await
    }

    /// Finish one scan generation. On success, remove the entries and sidecars
    /// it did not see and rebuild the groupings they fed; on failure, record
    /// the error and keep everything. `None` when `generation` is no longer
    /// the root's.
    pub async fn finish_folder_scan(
        &self,
        watched_folder_path: &Path,
        generation: u64,
        error: Option<&str>,
    ) -> Result<Option<FinishedScan>, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let generation = generation_column(generation)?;
        let error = error.map(str::to_string);
        if self.current_scan_generation(&watched_folder_path).await? != Some(generation) {
            return Ok(None);
        }
        self.store
            .call(move |sql| {
                ensure_generation(sql, &watched_folder_path, generation)?;
                if let Some(error) = error {
                    sql.execute(
                        "UPDATE folder_scan_roots SET status = 'failed', error = ? \
                     WHERE watched_folder_path = ? AND generation = ?",
                        params![error, coven::path_bytes(&watched_folder_path), generation],
                    )?;
                    return Ok(Some(FinishedScan::default()));
                }
                let pruned = write::prune_other_generations(sql, &watched_folder_path, generation)?;
                let pruned_sidecars =
                    sidecar::prune_sidecars(sql, &watched_folder_path, generation, None)?;
                sql.execute(
                    "UPDATE folder_scan_roots SET status = 'complete', error = NULL \
                 WHERE watched_folder_path = ? AND generation = ?",
                    params![coven::path_bytes(&watched_folder_path), generation],
                )?;
                let regrouped =
                    super::release_groupings::rebuild_groupings(sql, &pruned, &pruned_sidecars)?;
                Ok(Some(FinishedScan { pruned, regrouped }))
            })
            .await
    }

    /// Every stored entry under one watched root.
    pub async fn load_folder_scan_items(
        &self,
        watched_folder_path: &Path,
    ) -> Result<Vec<ScanItem>, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        self.store
            .read(move |sql| {
                load_folder_scan_items_on(&sql, &watched_folder_path, RowSources::Scanned)
            })
            .process(|process| process())
            .await
    }

    /// The stored entry at `entry_key`, whichever root it is under.
    pub async fn load_folder_scan_item(
        &self,
        entry_key: &CandidateKey,
    ) -> Result<Option<ScanItem>, DbError> {
        let entry_key = entry_key.clone();
        self.store
            .read(move |sql| read::load_item_by_key_rows(&sql, &entry_key, RowSources::Any))
            .process(|process| {
                process
                    .map(|process| {
                        let (root, stored) = process()?;
                        validate_scan_item_ownership(&root, &stored.key, &stored.item)?;
                        Ok(stored.item)
                    })
                    .transpose()
            })
            .await
    }
}

/// The stored entries `item` replaces: every other entry reading any of its
/// files, since two readings of one folder cannot both stand. A tentative
/// candidate replaces nothing; it is seen before the folders around it are
/// read, and the reading that settles them does the replacing.
fn superseded_keys(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    entry_key: &CandidateKey,
    item: &ScanItem,
) -> Result<Vec<CandidateKey>, DbError> {
    let (ScanItem::Valid(_) | ScanItem::Invalid(_)) = item else {
        return Ok(Vec::new());
    };
    read::entries_reading(
        sql,
        watched_folder_path,
        &item.coverage(),
        read::Readers::Besides(entry_key),
    )
}

/// What a pass hands the store for one folder, in the order the walk
/// yielded it.
pub(crate) enum FolderEntryToWrite {
    Release(ScanItemToWrite),
    Sidecar(crate::folder_scanner::FolderSidecar),
}

impl FolderEntryToWrite {
    /// The entry this writes.
    pub(crate) fn into_entry(self) -> crate::folder_scanner::FolderEntry {
        match self {
            Self::Release(to_write) => crate::folder_scanner::FolderEntry::Release(to_write.item),
            Self::Sidecar(sidecar) => crate::folder_scanner::FolderEntry::Sidecar(sidecar),
        }
    }
}

/// One release entry as a pass hands it to the store.
pub(crate) struct ScanItemToWrite {
    pub(crate) item: ScanItem,
    pub(crate) seed: ScanItemSeed,
    pub(crate) folder_date: Option<crate::folder_scanner::FolderDate>,
}

/// What one scan item is stored with, taken before the write.
pub(crate) enum ScanItemSeed {
    /// The reading of the folder's own tags, which its scan row keeps, and
    /// the draft and cover they project, which seed only a candidate with no
    /// draft yet: `None` for an item with no tracks, or whose tags could not
    /// be read.
    Tags(Option<crate::file_metadata_seed::FileMetadataSeed>),
    /// Nothing: a tentative candidate whose files a settled entry read when
    /// the seed was taken, which stores only when its folder was seen (see
    /// [`write_entry`]).
    AlreadySettled,
}

/// Write one scan item under `generation`, inside the caller's transaction,
/// deleting the entries it supersedes. The caller has checked `generation`.
fn write_scan_item(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    to_write: &FolderEntryToWrite,
    observed_at: i64,
    finding: Finding,
) -> Result<ScanItemWrite, DbError> {
    let to_write = match to_write {
        FolderEntryToWrite::Release(to_write) => to_write,
        FolderEntryToWrite::Sidecar(folder_sidecar) => {
            return sidecar::write_sidecar(sql, watched_folder_path, generation, folder_sidecar)
        }
    };
    let entry_key = to_write.item.key();
    validate_scan_item_ownership(watched_folder_path, &entry_key, &to_write.item)?;
    let EntryWrite::Stored { replaced: superseded_keys, found } = write_entry(
        sql,
        watched_folder_path,
        generation,
        to_write,
        EntrySource::Scanned { finding, observed_at },
    )?
    else {
        return Ok(ScanItemWrite::Unchanged);
    };
    // A valid release takes its files from any sidecar holding them.
    let uncovered = sidecar::delete_sidecars_holding(sql, &to_write.item)?;
    let touched: Vec<CandidateKey> =
        std::iter::once(entry_key).chain(superseded_keys.iter().cloned()).collect();
    let regrouped = super::release_groupings::rebuild_groupings(sql, &touched, &uncovered)?;
    Ok(ScanItemWrite::Stored { superseded_keys, regrouped, found })
}

/// Who writes an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntrySource {
    /// A scan, which replaces every other reading of its files and records
    /// the folder as seen at `observed_at`.
    Scanned { finding: Finding, observed_at: i64 },
    /// A grouping, which replaces only its own row. Its release is found when
    /// it is combined, and records no folder as seen: it is dated by the
    /// folders it takes in when the list reads it.
    Grouping,
}

/// What makes a valid release a scan stores found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Finding {
    /// Its folder had not been read as a release or as broken while a watched
    /// folder covered it.
    FirstRead,
    /// A person decided how its folder reads, and it was not listed before:
    /// the parts of a folder read apart again are found.
    Decision,
}

/// What writing one entry did.
pub(super) enum EntryWrite {
    /// The stored row already said exactly this.
    Unchanged,
    Stored {
        /// The keys of the entries it replaced.
        replaced: Vec<CandidateKey>,
        /// Whether it stored a valid release that is found, by the rule its
        /// [`EntrySource`] names.
        found: bool,
    },
}

/// Write one entry under `generation`, inside the caller's transaction.
pub(super) fn write_entry(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    to_write: &ScanItemToWrite,
    source: EntrySource,
) -> Result<EntryWrite, DbError> {
    let ScanItemToWrite { item, seed, folder_date } = to_write;
    let entry_key = item.key();
    let sources = match source {
        EntrySource::Scanned { .. } => RowSources::Scanned,
        EntrySource::Grouping => RowSources::Any,
    };
    let folder = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => candidate.path.as_path(),
        ScanItem::Invalid(candidate) => candidate.path.as_path(),
    };
    // Only a scan sees a folder. A grouping's release is dated by the folders
    // it takes in, so writing it must not record its folder as found now.
    let discovery = match source {
        EntrySource::Scanned { finding, observed_at } => Some((
            finding,
            dates::FolderDiscovery::observe(sql, folder, *folder_date, observed_at)?,
        )),
        EntrySource::Grouping => None,
    };
    let settles = !matches!(item, ScanItem::Discovered(_));
    let record_discovery = || match &discovery {
        Some((_, discovery)) => discovery.store(sql, settles),
        None => Ok(()),
    };
    let listed_before = read::settled_entry_is_stored(sql, watched_folder_path, &entry_key)?;
    // A rescan reports every candidate tentative before it reports it
    // settled, and a tentative candidate is for a folder seen for the first
    // time. One whose files a settled entry already reads — its own folder's
    // release, or the album its disc folders are read into — stores nothing
    // but when its folder was seen. Those entries keep standing, taking only
    // this generation's stamp, so they do not swing out of the list; the
    // settled reading that follows replaces them or keeps them.
    //
    // One seeded while a settled entry read its files stands down too when a
    // write before it in the same folder reading replaced that entry: every
    // tentative candidate a walk reports is followed by the settled reading
    // of its files, and nothing seeded it to be stored.
    if let ScanItem::Discovered(_) = item {
        let coverage = item.coverage();
        let settled =
            read::entries_reading(sql, watched_folder_path, &coverage, read::Readers::Settled)?;
        if !settled.is_empty() || matches!(seed, ScanItemSeed::AlreadySettled) {
            for key in &settled {
                write::touch_candidate(sql, watched_folder_path, key, generation)?;
            }
            record_discovery()?;
            return Ok(EntryWrite::Unchanged);
        }
    }
    let file_metadata = match seed {
        ScanItemSeed::Tags(file_metadata) => file_metadata,
        ScanItemSeed::AlreadySettled => {
            return Err(DbError::Message(format!(
                "{entry_key} is seeded as a tentative candidate a settled entry stands for, \
                 and it is not tentative"
            )))
        }
    };
    // An unchanged row only takes the stamp, so an untouched folder
    // announces nothing.
    let stored_item =
        read::load_item_by_key(sql, &entry_key, sources)?.map(|(_, stored)| stored.item);
    if stored_item.as_ref() == Some(item) {
        write::touch_candidate(sql, watched_folder_path, &entry_key, generation)?;
        record_discovery()?;
        return Ok(EntryWrite::Unchanged);
    }
    // Rewriting the row deletes its file-tag reading; a write with no
    // reading of its own keeps the stored one if the new row holds the
    // audio it was read from.
    let carried = match file_metadata.is_some() {
        true => None,
        false => tag_snapshot::load_file_tag_snapshot(sql, watched_folder_path, &entry_key)?
            .filter(|snapshot| item_was_read_for(item, snapshot)),
    };
    let removed_keys = match source {
        EntrySource::Scanned { .. } => superseded_keys(sql, watched_folder_path, &entry_key, item)?,
        EntrySource::Grouping => Vec::new(),
    };
    // An item is written over its own stored row, everything below that row
    // written anew; the entries it supersedes leave the scan.
    write::clear_entry_rows(sql, watched_folder_path, &entry_key)?;
    for key in &removed_keys {
        delete_entry(sql, watched_folder_path, key)?;
    }
    write::insert_item(sql, watched_folder_path, generation, item, file_metadata.as_ref(), source)?;
    // The candidate read with other files, or found invalid, is not what its
    // identification was of.
    let held = match item {
        ScanItem::Valid(candidate) | ScanItem::Discovered(candidate) => {
            Some(crate::CandidateFiles::of(candidate))
        }
        ScanItem::Invalid(_) => None,
    };
    super::import_state::end_file_work_unless_of_on(sql, &entry_key, held.as_ref())?;
    if let Some(snapshot) = carried {
        tag_snapshot::replace_candidate_file_tag_snapshot(
            sql,
            watched_folder_path,
            &entry_key,
            &snapshot,
        )?;
    }
    record_discovery()?;
    Ok(EntryWrite::Stored {
        replaced: removed_keys,
        found: matches!(item, ScanItem::Valid(_))
            && match &discovery {
                Some((Finding::FirstRead, discovery)) => !discovery.settled(),
                Some((Finding::Decision, _)) => !listed_before,
                None => false,
            },
    })
}

pub(super) fn load_folder_scan_items_on(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &Path,
    sources: RowSources,
) -> Result<impl FnOnce() -> Result<Vec<ScanItem>, DbError> + Send + 'static, DbError> {
    let items = read::load_items(sql, watched_folder_path, sources)?;
    let watched_folder_path = watched_folder_path.to_path_buf();
    Ok(move || {
        items()?
            .into_iter()
            .map(|stored| {
                validate_scan_item_ownership(&watched_folder_path, &stored.key, &stored.item)?;
                Ok(stored.item)
            })
            .collect()
    })
}

pub(super) fn validate_scan_item_ownership(
    watched_folder_path: &Path,
    entry_key: &CandidateKey,
    item: &ScanItem,
) -> Result<(), DbError> {
    if &item.key() != entry_key {
        return Err(DbError::Message(format!(
            "folder scan entry key {entry_key} does not match its item key {}",
            item.key()
        )));
    }
    let root = watched_folder_path;
    // A grouping's release is listed under its first member's watched folder
    // and may read folders under others; each release it takes in was checked
    // when stored.
    let grouped = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            candidate.grouping.is_some()
        }
        ScanItem::Invalid(candidate) => candidate.grouping.is_some(),
    };
    let (item_root, item_path) = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            (candidate.watched_folder_path.as_path(), candidate.path.clone())
        }
        ScanItem::Invalid(candidate) => {
            (candidate.watched_folder_path.as_path(), candidate.path.clone())
        }
    };
    if item_root.as_os_str() != watched_folder_path.as_os_str()
        || (!grouped && !item_path.starts_with(root))
    {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} does not belong to watched folder {}",
            coven::path_text(watched_folder_path)
        )));
    }
    if let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate), false) = (item, grouped) {
        if !candidate.file_root.starts_with(root)
            || candidate.files.parts.iter().any(|part| !part.folder.starts_with(root))
        {
            return Err(DbError::Message(format!(
                "folder scan entry {entry_key} reads files outside its watched folder"
            )));
        }
    }
    Ok(())
}

/// The next scan generation. An upsert, so a missing seed row is created
/// rather than failing the scan.
pub(super) fn next_folder_scan_generation(sql: &CovenWrite<'_>) -> Result<i64, DbError> {
    let generation: i64 = sql.query_row(
        "INSERT INTO folder_scan_generation_sequence (singleton, last_generation) \
         VALUES (1, 1) \
         ON CONFLICT(singleton) DO UPDATE SET last_generation = last_generation + 1 \
         RETURNING last_generation",
        [],
        |row| row.get(0),
    )?;
    Ok(generation)
}

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: the table of the directories each walk read
    /// is gone from the library under `directory`, so every read of what
    /// the last scan saw fails.
    pub(crate) async fn lose_scan_directories_for_test(directory: &Path) {
        let directory = directory.to_path_buf();
        tokio::task::spawn_blocking(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "folder_scan_directory",
                "DROP TABLE folder_scan_directory;",
            )
        })
        .await
        .unwrap();
    }
}

#[cfg(test)]
#[path = "folder_scans_tests.rs"]
pub(crate) mod tests;
