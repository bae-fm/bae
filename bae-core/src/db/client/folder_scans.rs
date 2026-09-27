//! The folder-scan tables: `folder_scan_roots`, `folder_scan_directory`, the
//! `scan_candidate` family and the `scan_sidecar` pair. A scan generation is
//! durable before traversal begins, each item deletes what it supersedes as it
//! is written, and successful completion prunes rows of other generations in
//! the transaction that marks the root complete.
//!
//! [`write`] stores candidate rows, [`read`] loads them back, and [`sidecar`]
//! stores a folder's sidecar files.

pub(super) mod columns;
mod dates;
mod progress;
pub(super) mod read;
mod reading;
mod sidecar;
pub(super) mod write;

use super::import_state::next_folder_scan_generation;
use super::query::{QueryOne, QueryRows};
use super::*;
use crate::import::folder_scanner::{FolderReleaseDecisionKey, ScanItem};
use std::path::{Path, PathBuf};

// `use super::*` also brings in the client's own `read` and `write`, hence `self::`.
pub(super) use self::read::{
    load_candidate_file_tag_snapshot, load_item_by_key, stored_entries, RowSources,
};
pub(super) use self::sidecar::load_sidecar;
pub(super) use self::write::{delete_entry, insert_candidate_files, StoredEntry};
pub(crate) use self::reading::{FolderReadingCommit, FolderReadingStamp, FolderReadingWrite};

/// What finishing a scan generation removed: the entries it did not see, and
/// the releases groupings rebuilt without them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FinishedScan {
    pub pruned: Vec<String>,
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
        superseded_keys: Vec<String>,
        regrouped: super::release_groupings::GroupingChanges,
        /// See [`EntryWrite::Stored`].
        found: bool,
    },
}

impl ScanItemWrite {
    /// Whether the row now says something it did not say before.
    pub fn changed(&self) -> bool {
        matches!(self, Self::Stored { .. })
    }

    /// The stored entries this write replaced. Empty when it wrote nothing.
    pub fn superseded_keys(&self) -> &[String] {
        match self {
            Self::Unchanged => &[],
            Self::Stored {
                superseded_keys, ..
            } => superseded_keys,
        }
    }

    /// The releases groupings rebuilt because of this write.
    pub fn regrouped(&self) -> Option<&super::release_groupings::GroupingChanges> {
        match self {
            Self::Unchanged => None,
            Self::Stored { regrouped, .. } => Some(regrouped),
        }
    }

    /// Whether this write stored a release for the first time.
    pub fn found(&self) -> bool {
        matches!(self, Self::Stored { found: true, .. })
    }
}

/// Refuse to write under a generation the root has moved past. Runs inside the
/// write transaction, so a scan that lost the root after the caller's check
/// cannot write over its successor.
fn ensure_generation(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
) -> Result<(), DbError> {
    let current: Option<i64> = sql
        .query_row(
            "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
            [watched_folder_path],
            |row| row.get(0),
        )
        .optional()?;
    if current != Some(generation) {
        return Err(DbError::Message(format!(
            "folder scan generation {generation} is no longer {watched_folder_path}'s: \
             a newer scan took the root between the check and the write"
        )));
    }
    Ok(())
}

/// Whether `snapshot` was taken from the audio `item` holds. Only a
/// tentative or valid candidate holds audio.
fn item_was_read_for(
    item: &ScanItem,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
) -> bool {
    match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            snapshot.was_read_from(candidate.files.audio())
        }
        ScanItem::Invalid(_) | ScanItem::Decided { .. } | ScanItem::Sidecar(_) => false,
    }
}

fn generation_column(generation: u64) -> Result<i64, DbError> {
    i64::try_from(generation).map_err(|_| {
        DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
    })
}

/// [`Database::replace_candidate_file_tag_snapshot`] inside the caller's
/// transaction.
pub(super) fn replace_candidate_file_tag_snapshot_on(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
) -> Result<bool, DbError> {
    let expected_generation = generation_column(snapshot.scan_generation)?;
    let expected_file_edit_revision = columns::to_i64(
        snapshot.file_edit_revision,
        "a file-tag snapshot's file edit revision",
    )?;
    let matched = sql.execute(
        "UPDATE scan_candidate SET generation = generation \
         WHERE watched_folder_path = ? AND path = ? \
           AND generation = ? AND file_edit_revision = ?",
        params![
            watched_folder_path,
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
        params![watched_folder_path, candidate_path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if audio_files.len() != snapshot.files.len()
        || audio_files
            .iter()
            .zip(&snapshot.files)
            .any(|((relative_path, size), fact)| {
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

    write::replace_candidate_file_tag_snapshot(sql, watched_folder_path, candidate_path, snapshot)?;
    Ok(true)
}

impl Database {
    /// The candidate's current scan generation and whatever file-tag snapshot
    /// is stored for it, even an outdated one, so a caller can tell a
    /// candidate never read from one whose reading is out of date.
    pub(crate) async fn load_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
    ) -> Result<Option<DbCandidateFileTagSnapshot>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        self.read(move |sql| {
            read::load_candidate_file_tag_snapshot(&sql, &watched_folder_path, &candidate_path)
        })
        .await
    }

    /// Replace a candidate's file-tag snapshot if its scan generation and file
    /// edit revision still match the snapshot's. `false` means the candidate
    /// changed first and nothing was written.
    pub(crate) async fn replace_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
    ) -> Result<bool, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let snapshot = snapshot.clone();
        self.call(move |sql| {
            replace_candidate_file_tag_snapshot_on(
                sql,
                &watched_folder_path,
                &candidate_path,
                &snapshot,
            )
        })
        .await
    }

    /// The root's generation, for a caller that records a failure against
    /// whichever generation stands now.
    pub(crate) async fn current_folder_scan_generation(
        &self,
        watched_folder_path: &str,
    ) -> Result<Option<u64>, DbError> {
        self.current_scan_generation(watched_folder_path)
            .await?
            .map(|generation| columns::to_u64(generation, "a folder scan root's generation"))
            .transpose()
    }

    /// The root's generation on the read connection, so a scan that lost the
    /// root finds out without opening a write.
    async fn current_scan_generation(
        &self,
        watched_folder_path: &str,
    ) -> Result<Option<i64>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            Ok(sql
                .query_row(
                    "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
                    [&watched_folder_path],
                    |row| row.get(0),
                )
                .optional()?)
        })
        .await
    }

    /// Open a new scan generation for `watched_folder_path`, recording the
    /// volume the folder is on.
    pub async fn begin_folder_scan(
        &self,
        watched_folder_path: &str,
        volume: crate::import::VolumeKind,
    ) -> Result<u64, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let volume = volume.as_column();
        self.call(move |sql| {
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
            u64::try_from(generation)
                .map_err(|_| DbError::Message("folder scan generation is negative".to_string()))
        })
        .await
    }

    /// Store one scan result, deleting the entries it supersedes in the same
    /// transaction. `None` when `generation` is no longer the root's.
    /// `file_metadata` is [`ScanItemToWrite::file_metadata`].
    pub(crate) async fn save_folder_scan_item_with_seed(
        &self,
        watched_folder_path: &str,
        generation: u64,
        item: &ScanItem,
        file_metadata: Option<crate::import::file_metadata_seed::FileMetadataSeed>,
        folder_date: Option<crate::import::folder_scanner::FolderDate>,
    ) -> Result<Option<ScanItemWrite>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = generation_column(generation)?;
        let item = item.clone();
        let observed_at = self.inner.clock.now().timestamp_millis();
        if self.current_scan_generation(&watched_folder_path).await? != Some(generation) {
            return Ok(None);
        }
        self.call(move |sql| {
            ensure_generation(sql, &watched_folder_path, generation)?;
            write_scan_item(
                sql,
                &watched_folder_path,
                generation,
                &ScanItemToWrite {
                    item,
                    file_metadata,
                    folder_date,
                },
                observed_at,
            )
            .map(Some)
        })
        .await
    }

    #[cfg(test)]
    pub async fn save_folder_scan_item(
        &self,
        watched_folder_path: &str,
        generation: u64,
        item: &ScanItem,
    ) -> Result<Option<ScanItemWrite>, DbError> {
        self.save_folder_scan_item_with_seed(watched_folder_path, generation, item, None, None)
            .await
    }

    /// Replace the directories recorded for `watched_folder_path` with those a
    /// completed walk read, and their mtimes. A walk that missed any mtime
    /// records none, and a root with nothing recorded is never taken to be
    /// unchanged.
    pub async fn record_folder_scan_directories(
        &self,
        watched_folder_path: &str,
        directories: &[(String, i64)],
    ) -> Result<(), DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let directories = directories.to_vec();
        self.call(move |sql| {
            sql.execute(
                "DELETE FROM folder_scan_directory WHERE watched_folder_path = ?",
                [&watched_folder_path],
            )?;
            for (path, modified_at) in &directories {
                sql.execute(
                    "INSERT INTO folder_scan_directory (watched_folder_path, path, modified_at) \
                     VALUES (?, ?, ?)",
                    params![watched_folder_path, path, modified_at],
                )?;
            }
            Ok(())
        })
        .await
    }

    /// The directories and mtimes recorded for this root; empty when none are.
    pub async fn load_folder_scan_directories(
        &self,
        watched_folder_path: &str,
    ) -> Result<Vec<(String, i64)>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            Ok(sql.query(
                "SELECT path, modified_at FROM folder_scan_directory \
                 WHERE watched_folder_path = ?",
                [watched_folder_path],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
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
        watched_folder_path: &str,
        generation: u64,
        error: Option<&str>,
    ) -> Result<Option<FinishedScan>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = generation_column(generation)?;
        let error = error.map(str::to_string);
        let observed_at = self.inner.clock.now().timestamp_millis();
        if self.current_scan_generation(&watched_folder_path).await? != Some(generation) {
            return Ok(None);
        }
        self.call(move |sql| {
            ensure_generation(sql, &watched_folder_path, generation)?;
            if let Some(error) = error {
                sql.execute(
                    "UPDATE folder_scan_roots SET status = 'failed', error = ? \
                     WHERE watched_folder_path = ? AND generation = ?",
                    params![error, watched_folder_path, generation],
                )?;
                return Ok(Some(FinishedScan::default()));
            }
            let pruned = write::prune_other_generations(sql, &watched_folder_path, generation)?;
            let pruned_sidecars =
                sidecar::prune_sidecars(sql, &watched_folder_path, generation, None)?;
            sql.execute(
                "UPDATE folder_scan_roots SET status = 'complete', error = NULL \
                 WHERE watched_folder_path = ? AND generation = ?",
                params![watched_folder_path, generation],
            )?;
            let regrouped = super::release_groupings::rebuild_groupings(
                sql,
                &pruned,
                &pruned_sidecars,
                observed_at,
            )?;
            Ok(Some(FinishedScan { pruned, regrouped }))
        })
        .await
    }

    #[cfg(test)]
    pub async fn load_folder_scan_snapshots(&self) -> Result<Vec<DbFolderScanSnapshot>, DbError> {
        self.read(move |sql| load_folder_scan_snapshots_on(&sql))
            .process(|process| process())
            .await
    }

    /// Every stored entry under one watched root.
    pub async fn load_folder_scan_items(
        &self,
        watched_folder_path: &str,
    ) -> Result<Vec<ScanItem>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            load_folder_scan_items_on(&sql, &watched_folder_path, RowSources::Scanned)
        })
            .process(|process| process())
            .await
    }

    /// Every stored entry under every watched root.
    pub async fn load_all_folder_scan_items(&self) -> Result<Vec<ScanItem>, DbError> {
        self.read(move |sql| {
            let roots = sql.query(
                "SELECT watched_folder_path FROM folder_scan_roots ORDER BY watched_folder_path",
                [],
                |row| row.get::<_, String>(0),
            )?;
            let mut items = Vec::new();
            for root in roots {
                items.push(load_folder_scan_items_on(&sql, &root, RowSources::Any)?);
            }
            Ok(items)
        })
        .process(|roots| {
            let mut items = Vec::new();
            for root in roots {
                items.extend(root()?);
            }
            Ok(items)
        })
        .await
    }

    /// The stored entry at `entry_key`, whichever root it is under.
    pub async fn load_folder_scan_item(
        &self,
        entry_key: &str,
    ) -> Result<Option<ScanItem>, DbError> {
        let entry_key = entry_key.to_string();
        self.read(move |sql| read::load_item_by_key_rows(&sql, &entry_key, RowSources::Any))
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
fn superseded_keys(stored: &[StoredEntry], item: &ScanItem) -> Vec<String> {
    let (ScanItem::Valid(_) | ScanItem::Invalid(_)) = item else {
        return Vec::new();
    };
    let (Some(own_key), Some(coverage)) = (item.persisted_key(), item.coverage()) else {
        return Vec::new();
    };
    stored
        .iter()
        .filter(|entry| entry.key != own_key && entry.coverage.overlaps(&coverage))
        .map(|entry| entry.key.clone())
        .collect()
}

/// One scan item as a pass hands it to the store.
pub(crate) struct ScanItemToWrite {
    pub(crate) item: ScanItem,
    /// The draft the folder's own tags project, the reading it came from, and
    /// the cover those tags embed. Seeds only a candidate with no draft yet.
    pub(crate) file_metadata: Option<crate::import::file_metadata_seed::FileMetadataSeed>,
    pub(crate) folder_date: Option<crate::import::folder_scanner::FolderDate>,
}

/// Write one scan item under `generation`, inside the caller's transaction,
/// deleting the entries it supersedes. The caller has checked `generation`.
fn write_scan_item(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    to_write: &ScanItemToWrite,
    observed_at: i64,
) -> Result<ScanItemWrite, DbError> {
    if let ScanItem::Sidecar(folder_sidecar) = &to_write.item {
        return sidecar::write_sidecar(
            sql,
            watched_folder_path,
            generation,
            folder_sidecar,
            observed_at,
        );
    }
    let Some(entry_key) = to_write.item.persisted_key() else {
        return Err(DbError::Message(
            "a folder reading is stored as a decision, not as a scan entry".to_string(),
        ));
    };
    validate_scan_item_ownership(watched_folder_path, &entry_key, &to_write.item)?;
    let EntryWrite::Stored {
        replaced: superseded_keys,
        found,
    } = write_entry(
        sql,
        watched_folder_path,
        generation,
        to_write,
        observed_at,
        EntrySource::Scanned,
    )?
    else {
        return Ok(ScanItemWrite::Unchanged);
    };
    // A valid release takes its files from any sidecar holding them.
    let uncovered = sidecar::delete_sidecars_holding(sql, &to_write.item)?;
    let touched: Vec<String> = std::iter::once(entry_key)
        .chain(superseded_keys.iter().cloned())
        .collect();
    let regrouped =
        super::release_groupings::rebuild_groupings(sql, &touched, &uncovered, observed_at)?;
    Ok(ScanItemWrite::Stored {
        superseded_keys,
        regrouped,
        found,
    })
}

/// Who writes an entry: a scan, which replaces every other reading of its
/// files, or a grouping, which replaces only its own row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntrySource {
    Scanned,
    Grouping,
}

/// What writing one entry did.
pub(super) enum EntryWrite {
    /// The stored row already said exactly this.
    Unchanged,
    Stored {
        /// The keys of the entries it replaced.
        replaced: Vec<String>,
        /// Whether a scan read a valid release in a folder no scan had read
        /// as a release or as broken; new files in a known folder are not a
        /// new release.
        found: bool,
    },
}

/// Write one entry under `generation`, inside the caller's transaction.
pub(super) fn write_entry(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    to_write: &ScanItemToWrite,
    observed_at: i64,
    source: EntrySource,
) -> Result<EntryWrite, DbError> {
    let ScanItemToWrite {
        item,
        file_metadata,
        folder_date,
    } = to_write;
    let Some(entry_key) = item.persisted_key() else {
        return Err(DbError::Message(
            "a folder reading is stored as a decision, not as a scan entry".to_string(),
        ));
    };
    let sources = match source {
        EntrySource::Scanned => RowSources::Scanned,
        EntrySource::Grouping => RowSources::Any,
    };
    let folder = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => candidate.path.as_path(),
        ScanItem::Invalid(candidate) => candidate.path.as_path(),
        ScanItem::Decided { .. } | ScanItem::Sidecar(_) => {
            return Err(DbError::Message(format!(
                "{entry_key} is not a release, so it is not stored as a scan entry"
            )))
        }
    };
    let discovery = dates::FolderDiscovery::observe(sql, folder, *folder_date, observed_at)?;
    // A grouping's release is found when it is combined, not by the folder
    // it is listed at.
    let settles = source == EntrySource::Scanned && !matches!(item, ScanItem::Discovered(_));
    // A rescan reports every candidate tentative before it reports it settled.
    // A settled row keeps standing through that, taking only this
    // generation's stamp, so it does not swing out of the list; the settled
    // write that follows replaces it.
    if matches!(item, ScanItem::Discovered(_))
        && read::settled_entry_is_stored(sql, watched_folder_path, &entry_key)?
    {
        write::touch_candidate(sql, watched_folder_path, &entry_key, generation)?;
        discovery.store(sql, settles)?;
        return Ok(EntryWrite::Unchanged);
    }
    // An unchanged row only takes the stamp, so an untouched folder
    // announces nothing.
    let stored_item = read::load_item_by_key(sql, &entry_key, sources)?.map(|(_, stored)| stored.item);
    if stored_item.as_ref() == Some(item) {
        write::touch_candidate(sql, watched_folder_path, &entry_key, generation)?;
        discovery.store(sql, settles)?;
        return Ok(EntryWrite::Unchanged);
    }
    // Rewriting the row deletes its file-tag reading; a write with no
    // reading of its own keeps the stored one if the new row holds the
    // audio it was read from.
    let carried = match file_metadata.is_some() {
        true => None,
        false => read::load_file_tag_snapshot(sql, watched_folder_path, &entry_key)?
            .filter(|snapshot| item_was_read_for(item, snapshot)),
    };
    let removed_keys = match source {
        EntrySource::Scanned => superseded_keys(&stored_entries(sql, watched_folder_path)?, item),
        EntrySource::Grouping => Vec::new(),
    };
    // An item replaces its own stored row whole.
    for key in std::iter::once(&entry_key).chain(removed_keys.iter()) {
        delete_entry(sql, watched_folder_path, key)?;
    }
    write::insert_item(
        sql,
        watched_folder_path,
        generation,
        item,
        file_metadata.as_ref(),
        source,
    )?;
    if let Some(snapshot) = carried {
        write::replace_candidate_file_tag_snapshot(
            sql,
            watched_folder_path,
            &entry_key,
            &snapshot,
        )?;
    }
    discovery.store(sql, settles)?;
    Ok(EntryWrite::Stored {
        replaced: removed_keys,
        found: settles && matches!(item, ScanItem::Valid(_)) && !discovery.settled(),
    })
}

pub(super) fn load_folder_scan_items_on(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
    sources: RowSources,
) -> Result<impl FnOnce() -> Result<Vec<ScanItem>, DbError> + Send + 'static, DbError> {
    let items = read::load_items(sql, watched_folder_path, sources)?;
    let watched_folder_path = watched_folder_path.to_string();
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

#[cfg(test)]
fn load_folder_scan_snapshots_on(
    sql: &SqlReadContext<'_>,
) -> Result<impl FnOnce() -> Result<Vec<DbFolderScanSnapshot>, DbError> + Send + 'static, DbError> {
    let roots = sql.query(
        "SELECT roots.watched_folder_path, roots.generation, roots.status, roots.error, \
                COUNT(candidate.path) \
         FROM folder_scan_roots AS roots \
         LEFT JOIN scan_candidate AS candidate \
           ON candidate.watched_folder_path = roots.watched_folder_path \
          AND candidate.generation = roots.generation \
         GROUP BY roots.watched_folder_path, roots.generation, roots.status, roots.error \
         ORDER BY roots.watched_folder_path",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;

    let mut snapshots = Vec::with_capacity(roots.len());
    for (watched_folder_path, generation, status, error, found_count) in roots {
        let generation = u64::try_from(generation).map_err(|_| {
            DbError::Message(format!(
                "folder scan root {watched_folder_path} has a negative generation"
            ))
        })?;
        let status = match (status.as_str(), error) {
            ("scanning", None) => crate::import::FolderScanStatus::Scanning {
                found_count: columns::to_u64(found_count, "current folder-scan candidate count")?,
            },
            ("complete", None) => crate::import::FolderScanStatus::Complete,
            ("failed", Some(error)) => crate::import::FolderScanStatus::Failed { error },
            (status, error) => {
                return Err(DbError::Message(format!(
                    "folder scan root {watched_folder_path} has invalid status {status:?} \
                     and error {error:?}"
                )))
            }
        };
        let items = load_folder_scan_items_on(sql, &watched_folder_path, RowSources::Any)?;
        snapshots.push((watched_folder_path, generation, status, items));
    }
    Ok(move || {
        snapshots
            .into_iter()
            .map(|(watched_folder_path, generation, status, items)| {
                Ok(DbFolderScanSnapshot {
                    watched_folder_path,
                    generation,
                    status,
                    items: items()?,
                })
            })
            .collect()
    })
}

pub(super) fn validate_scan_item_ownership(
    watched_folder_path: &str,
    entry_key: &str,
    item: &ScanItem,
) -> Result<(), DbError> {
    if item.persisted_key().as_deref() != Some(entry_key) {
        return Err(DbError::Message(format!(
            "folder scan entry key {entry_key} does not match its item key {:?}",
            item.persisted_key()
        )));
    }
    let root = Path::new(watched_folder_path);
    // A grouping's release is listed under its first member's watched folder
    // and may read folders under others; each release it takes in was checked
    // when stored.
    let grouped = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            candidate.grouping.is_some()
        }
        ScanItem::Invalid(candidate) => candidate.grouping.is_some(),
        ScanItem::Decided { .. } | ScanItem::Sidecar(_) => false,
    };
    let (item_root, item_path) = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        ScanItem::Invalid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        ScanItem::Decided { key, .. } => (key.watched_folder_path.as_str(), Path::new(entry_key)),
        ScanItem::Sidecar(sidecar) => (
            sidecar.watched_folder_path.as_str(),
            sidecar.folder.as_path(),
        ),
    };
    if item_root != watched_folder_path || (!grouped && !item_path.starts_with(root)) {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} does not belong to watched folder {watched_folder_path}"
        )));
    }
    match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) if !grouped => {
            if !candidate.file_root.starts_with(root)
                || candidate
                    .files
                    .parts
                    .iter()
                    .any(|part| !part.folder.starts_with(root))
            {
                return Err(DbError::Message(format!(
                    "folder scan entry {entry_key} reads files outside its watched folder"
                )));
            }
        }
        ScanItem::Discovered(_)
        | ScanItem::Valid(_)
        | ScanItem::Invalid(_)
        | ScanItem::Sidecar(_) => {}
        ScanItem::Decided { key, .. } => {
            validate_decision_key_ownership(watched_folder_path, key)?;
        }
    }
    Ok(())
}

/// Store the grouping `grouping` anchored at the folder `key` names; a
/// heuristic reading never replaces the person's. A folder's grouping keeps
/// its key while stored, since its release is addressed by it, so a reading
/// naming a different key is stale and refused.
pub(super) fn store_folder_reading(
    sql: &SqlContext<'_, '_>,
    key: &FolderReleaseDecisionKey,
    decision: crate::import::folder_scanner::FolderReleaseDecision,
    author: crate::import::folder_scanner::FolderReleaseDecisionAuthor,
    grouping: &str,
) -> Result<(), DbError> {
    crate::import::watched_folder::validate_relative_path(&key.relative_folder_path)?;
    if key.relative_folder_path.is_empty() {
        return Err(DbError::Message(format!(
            "{} is never a release, so it has no reading to store",
            key.watched_folder_path
        )));
    }
    let author = match author {
        crate::import::folder_scanner::FolderReleaseDecisionAuthor::User => "user",
        crate::import::folder_scanner::FolderReleaseDecisionAuthor::Heuristic => "heuristic",
    };
    let combined = matches!(
        decision,
        crate::import::folder_scanner::FolderReleaseDecision::CombineAsOneRelease
    );
    let anchor = crate::import::watched_folder::folder_below(
        &key.watched_folder_path,
        &key.relative_folder_path,
    )?;
    let covering = super::import_state::require_watched(sql, &anchor)?;
    if covering != key.watched_folder_path {
        return Err(DbError::Message(format!(
            "{anchor} is watched under {covering}, not {}",
            key.watched_folder_path
        )));
    }
    sql.execute(
        "INSERT INTO release_grouping (key, anchor_folder, combined, author) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(anchor_folder) DO UPDATE SET \
             combined = excluded.combined, author = excluded.author \
         WHERE excluded.author = 'user' \
             OR release_grouping.author != 'user'",
        params![grouping, anchor, combined, author],
    )?;
    let stored: String = sql.query_row(
        "SELECT key FROM release_grouping WHERE anchor_folder = ?",
        [&anchor],
        |row| row.get(0),
    )?;
    if stored != grouping {
        return Err(DbError::Message(format!(
            "{} is read as grouping {stored}, not {grouping}: the store changed after the \
             reading was taken",
            key.relative_folder_path
        )));
    }
    Ok(())
}

pub(super) fn validate_decision_key_ownership(
    watched_folder_path: &str,
    key: &FolderReleaseDecisionKey,
) -> Result<(), DbError> {
    if key.watched_folder_path != watched_folder_path {
        return Err(DbError::Message(format!(
            "folder release decision belongs to {} instead of {watched_folder_path}",
            key.watched_folder_path
        )));
    }
    Ok(crate::import::watched_folder::validate_relative_path(
        &key.relative_folder_path,
    )?)
}
