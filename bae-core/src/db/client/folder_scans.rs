//! The durable folder-scan tables: `folder_scan_roots`, the `scan_candidate`
//! family, and the `scan_boundary` family. A scan generation is durable before
//! traversal begins; items are written as they are discovered, each deleting
//! what it supersedes; successful completion prunes rows not written in that
//! generation in the same transaction that marks the root complete.
//!
//! One [`ScanItem`](crate::import::folder_scanner::ScanItem) is a candidate row
//! with its files, their parsed track sheets and the decisions that exposed it
//! — or a boundary row with its tree and the candidates it hides. [`write`]
//! lays those rows down and [`read`] assembles them back.

pub(super) mod columns;
mod dates;
mod progress;
pub(super) mod read;
mod reading;
pub(super) mod write;

use super::import_state::next_folder_scan_generation;
use super::query::{QueryOne, QueryRows};
use super::*;
use crate::import::folder_scanner::{FolderReleaseDecisionKey, ScanItem};
use std::path::{Path, PathBuf};

// `use super::*` above also brings the client's own `read` and `write` modules
// into scope, so these name this module's pair explicitly.
pub(super) use self::read::{
    load_candidate_file_tag_snapshot, load_item_by_key, stored_entries, RowSources,
};
pub(super) use self::write::{delete_entry, insert_candidate_files, StoredEntry};
pub(crate) use self::reading::{FolderReadingCommit, FolderReadingStamp, FolderReadingWrite};

/// What finishing a scan generation removed: the entries it did not see, and
/// the releases groupings rebuilt without them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FinishedScan {
    pub pruned: Vec<String>,
    pub regrouped: super::release_groupings::GroupingChanges,
}

/// What one scan item's write did.
///
/// The distinction the import list lives on: a pass that finds a folder exactly
/// as it left it has nothing to tell anyone, and saying so is what keeps a
/// timer-driven re-read of a watched folder free.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanItemWrite {
    /// The stored row already said exactly this. It kept its place and took
    /// this generation's stamp, so the completion prune keeps it too.
    Unchanged,
    /// The row was written, displacing the keys named here, and rebuilding
    /// the releases that groupings build from what it changed.
    Stored {
        superseded_keys: Vec<String>,
        regrouped: super::release_groupings::GroupingChanges,
    },
}

impl ScanItemWrite {
    /// Whether the row now says something it did not say before — the only
    /// case anyone has to hear about.
    pub fn changed(&self) -> bool {
        matches!(self, Self::Stored { .. })
    }

    /// The stored entries this write displaced. Empty when it wrote nothing.
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
}

/// Refuse to write under a generation the root has moved past. Read inside the
/// write transaction, so a scan that lost the root between the caller's check
/// and this write is refused rather than writing over its successor.
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

/// Whether `snapshot` is a reading of `item` — the audio it was taken from is
/// the audio `item` holds. An invalid candidate carries no files, so no
/// reading is ever a reading of one.
fn item_was_read_for(
    item: &ScanItem,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
) -> bool {
    match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            snapshot.was_read_from(candidate.files.audio())
        }
        ScanItem::Invalid(_) | ScanItem::Decided { .. } => false,
    }
}

fn generation_column(generation: u64) -> Result<i64, DbError> {
    i64::try_from(generation).map_err(|_| {
        DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
    })
}

/// Store one complete reading only under the source stamp it describes.
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
    /// Load the candidate's current scan stamp and whatever complete file-tag
    /// snapshot is stored beneath it. The two stamps are deliberately not
    /// collapsed: a caller must distinguish never-read from invalidated.
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

    /// Atomically replace a candidate's complete file-tag snapshot if its
    /// durable scan generation and file-decision revision still match what was
    /// read. `false` means the candidate moved before the write; nothing was
    /// deleted or inserted.
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

    /// The root's generation as the read connection sees it. A scan that is no
    /// longer the root's writes nothing, and finding that out is a read — the
    /// writes below open only once this generation is the one in force.
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

    /// Start a durable scan generation for one watched root.
    /// Open a new scan generation for `watched_folder_path`, recording the
    /// volume the folder is on as this scan found it.
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

    /// Persist one progressive scan result.
    ///
    /// The entries `item` supersedes under its root — what a resolved boundary
    /// hid, the tentative rows a boundary hides — are found and deleted in the
    /// same transaction, and returned so the caller can announce them. `None`
    /// when `generation` is no longer the root's: the generation check and all
    /// changes share one transaction, so a cancelled scan cannot write over
    /// its successor.
    /// `file_metadata` seeds a candidate this scan is storing for the first time:
    /// the draft the folder's own tags project, the reading it came from, and
    /// the cover those tags embed. A candidate that already has a draft keeps
    /// it — a rescan re-reads files, not decisions.
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

    /// Record every directory a completed walk of `watched_folder_path` read,
    /// with the mtime it had, replacing whatever the last walk recorded.
    ///
    /// An empty list clears the root: a walk that could not read some
    /// directory's mtime records nothing rather than a partial picture, and a
    /// root with nothing recorded is one the cheap check refuses to answer for.
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

    /// Every directory the last completed walk of this root recorded, with the
    /// mtime it had. Empty when no walk has recorded any.
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

    /// Finish one scan generation. Successful completion removes entries not
    /// observed in this generation and returns their keys; failure preserves
    /// them. `None` when `generation` is no longer the root's.
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
            sql.execute(
                "UPDATE folder_scan_roots SET status = 'complete', error = NULL \
                 WHERE watched_folder_path = ? AND generation = ?",
                params![watched_folder_path, generation],
            )?;
            let regrouped =
                super::release_groupings::rebuild_groupings(sql, &pruned, observed_at)?;
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

    /// The stored entry at `entry_key`, whichever root it is under. Watched
    /// roots never overlap and keys are absolute paths, so at most one root
    /// holds it.
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

/// The stored entries `item` replaces when it is written: every other entry
/// reading any of the files it reads. Two readings of one folder cannot both
/// stand — a folder read as one release replaces the releases below it, and a
/// release below a folder once read as one replaces that reading.
///
/// A tentative candidate replaces nothing: it is seen before the folders
/// around it are understood, and the reading that settles them is what
/// replaces whatever it contradicts.
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

/// One scan item as a pass hands it to the store: the entry, the file-tag
/// reading that seeds a candidate stored for the first time, and the date the
/// folder carries.
pub(crate) struct ScanItemToWrite {
    pub(crate) item: ScanItem,
    /// The draft the folder's own tags project, the reading it came from, and
    /// the cover those tags embed. A candidate that already has a draft keeps
    /// it — a rescan re-reads files, not decisions.
    pub(crate) file_metadata: Option<crate::import::file_metadata_seed::FileMetadataSeed>,
    pub(crate) folder_date: Option<crate::import::folder_scanner::FolderDate>,
}

/// Write one scan item under `generation`, inside the caller's transaction,
/// deleting the entries it supersedes. The caller has already checked that
/// `generation` is the one this write may stamp.
fn write_scan_item(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    to_write: &ScanItemToWrite,
    observed_at: i64,
) -> Result<ScanItemWrite, DbError> {
    let Some(entry_key) = to_write.item.persisted_key() else {
        return Err(DbError::Message(
            "a folder reading is stored as a decision, not as a scan entry".to_string(),
        ));
    };
    validate_scan_item_ownership(watched_folder_path, &entry_key, &to_write.item)?;
    let written = write_entry(
        sql,
        watched_folder_path,
        generation,
        to_write,
        observed_at,
        EntrySource::Scanned,
    )?;
    let Some(superseded_keys) = written else {
        return Ok(ScanItemWrite::Unchanged);
    };
    let touched: Vec<String> = std::iter::once(entry_key)
        .chain(superseded_keys.iter().cloned())
        .collect();
    let regrouped = super::release_groupings::rebuild_groupings(sql, &touched, observed_at)?;
    Ok(ScanItemWrite::Stored {
        superseded_keys,
        regrouped,
    })
}

/// Who writes an entry: a scan, which reads the folder and replaces whatever
/// other reading of its files stood; or a grouping, which builds its release
/// from releases the scans wrote and replaces nothing but its own row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntrySource {
    Scanned,
    Grouping,
}

/// Write one entry under `generation`, inside the caller's transaction. `None`
/// when the stored row already said exactly this; otherwise the keys of the
/// entries it replaced.
pub(super) fn write_entry(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    to_write: &ScanItemToWrite,
    observed_at: i64,
    source: EntrySource,
) -> Result<Option<Vec<String>>, DbError> {
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
    let discovery = dates::CandidateDiscovery::observe(
        sql,
        watched_folder_path,
        &entry_key,
        *folder_date,
        observed_at,
    )?;
    // A re-walk rewrites every candidate it finds, and each one arrives
    // tentative before it arrives valid — tentative meaning "seen
    // before its enclosing folder was understood". A row that is
    // already a settled release has been understood; sending it back
    // through that window would take it out of the list and the tab
    // counts until the valid write lands a moment later, which is the
    // swing a viewer sees while a folder rescans. The stored row
    // stands and only takes this generation's stamp, so the completion
    // prune keeps it; the valid write that follows replaces it whole.
    //
    // A candidate this scan is seeing for the first time has nothing
    // stored, so it still appears tentative — which is the only thing
    // tentative is for. A row this scan decides is hidden after all is
    // removed by the reading that hides it, which supersedes by the files
    // it reads and does not care which kind the row was.
    if matches!(item, ScanItem::Discovered(_))
        && read::candidate_is_valid(sql, watched_folder_path, &entry_key)?
    {
        write::touch_candidate(sql, watched_folder_path, &entry_key, generation)?;
        discovery.store(sql, watched_folder_path, &entry_key)?;
        return Ok(None);
    }
    // A walk of a folder nobody has touched produces exactly the items
    // already stored for it. Rewriting one of those would mean a
    // transaction, an announcement, and every reader of the import list
    // rebuilding it — per row, per pass, forever, over a folder that did
    // not change. So the row keeps its place and takes only this
    // generation's stamp, which is all the completion prune asks of it.
    let stored_item = read::load_item_by_key(sql, &entry_key, sources)?.map(|(_, stored)| stored.item);
    if stored_item.as_ref() == Some(item) {
        write::touch_candidate(sql, watched_folder_path, &entry_key, generation)?;
        discovery.store(sql, watched_folder_path, &entry_key)?;
        return Ok(None);
    }
    // Rewriting the row takes the file-tag reading hanging off it, and
    // the draft that reading projected outlives the rewrite. So a write
    // that brings no reading of its own carries the stored one across —
    // onto a row that still holds the files it was read from, and only
    // there. A folder that now fails validation holds no files at all,
    // and one whose audio changed holds other files; either way the
    // reading describes what the row no longer is, and it goes with the
    // row it belonged to.
    let carried = match file_metadata.is_some() {
        true => None,
        false => read::load_file_tag_snapshot(sql, watched_folder_path, &entry_key)?
            .filter(|snapshot| item_was_read_for(item, snapshot)),
    };
    let removed_keys = match source {
        EntrySource::Scanned => superseded_keys(&stored_entries(sql, watched_folder_path)?, item),
        EntrySource::Grouping => Vec::new(),
    };
    // The item's own prior row goes first: an item is written whole,
    // so what stood under its key is replaced rather than merged with.
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
    discovery.store(sql, watched_folder_path, &entry_key)?;
    Ok(Some(removed_keys))
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
    // A release a grouping builds is listed under one watched folder and may
    // read folders under others; what it reads was checked when each release
    // it takes in was stored.
    let grouped = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            candidate.grouping.is_some()
        }
        ScanItem::Invalid(candidate) => candidate.grouping.is_some(),
        ScanItem::Decided { .. } => false,
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
        ScanItem::Discovered(_) | ScanItem::Valid(_) | ScanItem::Invalid(_) => {}
        ScanItem::Decided { key, .. } => {
            validate_decision_key_ownership(watched_folder_path, key)?;
        }
    }
    Ok(())
}

/// Store how the folder at `key` reads, as the grouping `grouping`. Either
/// author's answer lands where nothing is stored; a scan's own reading never
/// replaces the person's.
///
/// A folder's grouping keeps one key for as long as it is stored, because the
/// release it reads as is addressed by that key. A reading that names another
/// key than the one stored at the folder was taken against a store that has
/// since changed, and is refused.
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
    sql.execute(
        "INSERT INTO release_grouping \
             (key, watched_folder_path, anchor_relative_path, combined, author) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(watched_folder_path, anchor_relative_path) DO UPDATE SET \
             combined = excluded.combined, author = excluded.author \
         WHERE excluded.author = 'user' \
             OR release_grouping.author != 'user'",
        params![
            grouping,
            key.watched_folder_path,
            key.relative_folder_path,
            combined,
            author
        ],
    )?;
    let stored: String = sql.query_row(
        "SELECT key FROM release_grouping \
         WHERE watched_folder_path = ? AND anchor_relative_path = ?",
        params![key.watched_folder_path, key.relative_folder_path],
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
