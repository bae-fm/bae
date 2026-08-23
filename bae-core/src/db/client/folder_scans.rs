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
mod read;
mod write;

use super::import_state::next_folder_scan_generation;
use super::query::{QueryOne, QueryRows};
use super::*;
use crate::import::candidates::StoredEntryKey;
use crate::import::folder_scanner::{FolderReleaseDecisionKey, ScanItem};
use read::StoredScanItem;
use std::path::{Path, PathBuf};

pub(super) use read::{
    load_boundary_items, load_candidate_items, load_item_by_key, load_resolved_boundaries,
    stored_entries,
};
pub(super) use write::{delete_entry, insert_candidate_files, StoredEntry};

/// The entry at `entry_key`, on whichever connection the caller holds — the
/// read connection for a query, the write transaction for a decision that has
/// just reshaped it.
pub(super) fn load_scan_item_on(
    sql: &(impl QueryOne + QueryRows),
    entry_key: &str,
) -> Result<Option<ScanItem>, DbError> {
    let Some((watched_folder_path, stored)) = read::load_item_by_key(sql, entry_key)? else {
        return Ok(None);
    };
    validate_scan_item_ownership(&watched_folder_path, &stored.key, &stored.item)?;
    Ok(Some(stored.item))
}

/// The key a boundary is addressed by — its watched root joined with the
/// folder it asks about, which is what
/// [`ScanItem::persisted_key`](crate::import::folder_scanner::ScanItem) builds
/// from the same two fields.
pub(super) fn boundary_key(watched_folder_path: &str, relative_folder_path: &str) -> String {
    Path::new(watched_folder_path)
        .join(relative_folder_path)
        .to_string_lossy()
        .into_owned()
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

fn generation_column(generation: u64) -> Result<i64, DbError> {
    i64::try_from(generation).map_err(|_| {
        DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
    })
}

impl Database {
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
    pub async fn begin_folder_scan(&self, watched_folder_path: &str) -> Result<u64, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.call(move |sql| {
            let generation = next_folder_scan_generation(sql)?;
            sql.execute(
                "INSERT INTO folder_scan_roots \
                     (watched_folder_path, generation, status, error) \
                 VALUES (?, ?, 'scanning', NULL) \
                 ON CONFLICT(watched_folder_path) DO UPDATE SET \
                     generation = excluded.generation, status = 'scanning', error = NULL",
                params![watched_folder_path, generation],
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
    pub async fn save_folder_scan_item(
        &self,
        watched_folder_path: &str,
        generation: u64,
        item: &ScanItem,
    ) -> Result<Option<Vec<String>>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = generation_column(generation)?;
        let entry_key = item.persisted_key();
        validate_scan_item_ownership(&watched_folder_path, &entry_key, item)?;
        let item = item.clone();
        if self.current_scan_generation(&watched_folder_path).await? != Some(generation) {
            return Ok(None);
        }
        self.call(move |sql| {
            ensure_generation(sql, &watched_folder_path, generation)?;
            let stored = stored_entries(sql, &watched_folder_path)?;
            let keys: Vec<StoredEntryKey> = stored
                .iter()
                .map(|(key, entry)| StoredEntryKey {
                    key: key.clone(),
                    is_boundary: matches!(entry, StoredEntry::Boundary { .. }),
                })
                .collect();
            let removed_keys = crate::import::candidates::superseded_entry_keys(&keys, &item);
            let stored: HashMap<&str, &StoredEntry> = stored
                .iter()
                .map(|(key, entry)| (key.as_str(), entry))
                .collect();
            // The item's own prior row goes first: an item is written whole,
            // so what stood under its key is replaced rather than merged with.
            for key in std::iter::once(&entry_key).chain(removed_keys.iter()) {
                if let Some(entry) = stored.get(key.as_str()) {
                    delete_entry(sql, &watched_folder_path, entry)?;
                }
            }
            write::insert_item(sql, &watched_folder_path, generation, &item)?;
            Ok(Some(removed_keys))
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
    ) -> Result<Option<Vec<String>>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = generation_column(generation)?;
        let error = error.map(str::to_string);
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
                return Ok(Some(Vec::new()));
            }
            let pruned = write::prune_other_generations(sql, &watched_folder_path, generation)?;
            sql.execute(
                "UPDATE folder_scan_roots SET status = 'complete', error = NULL \
                 WHERE watched_folder_path = ? AND generation = ?",
                params![watched_folder_path, generation],
            )?;
            Ok(Some(pruned))
        })
        .await
    }

    pub async fn load_folder_scan_snapshots(&self) -> Result<Vec<DbFolderScanSnapshot>, DbError> {
        self.read(move |sql| load_folder_scan_snapshots_on(&sql))
            .await
    }

    /// Every stored entry under one watched root.
    pub async fn load_folder_scan_items(
        &self,
        watched_folder_path: &str,
    ) -> Result<Vec<ScanItem>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| load_folder_scan_items_on(&sql, &watched_folder_path))
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
                items.extend(load_folder_scan_items_on(&sql, &root)?);
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
        self.read(move |sql| load_scan_item_on(&sql, &entry_key))
            .await
    }
}

pub(super) fn load_folder_scan_items_on(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
) -> Result<Vec<ScanItem>, DbError> {
    read::load_items(sql, watched_folder_path)?
        .into_iter()
        .map(|stored| {
            validate_scan_item_ownership(watched_folder_path, &stored.key, &stored.item)?;
            Ok(stored.item)
        })
        .collect()
}

pub(super) fn load_folder_scan_snapshots_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<DbFolderScanSnapshot>, DbError> {
    let roots = sql.query(
        "SELECT watched_folder_path, generation, status, error \
         FROM folder_scan_roots ORDER BY watched_folder_path",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;

    let mut snapshots = Vec::with_capacity(roots.len());
    for (watched_folder_path, generation, status, error) in roots {
        let generation = u64::try_from(generation).map_err(|_| {
            DbError::Message(format!(
                "folder scan root {watched_folder_path} has a negative generation"
            ))
        })?;
        let status = match (status.as_str(), error) {
            ("scanning", None) => crate::import::FolderScanStatus::Scanning,
            ("complete", None) => crate::import::FolderScanStatus::Complete,
            ("failed", Some(error)) => crate::import::FolderScanStatus::Failed { error },
            (status, error) => {
                return Err(DbError::Message(format!(
                    "folder scan root {watched_folder_path} has invalid status {status:?} \
                     and error {error:?}"
                )))
            }
        };
        let stored = read::load_items(sql, &watched_folder_path)?;
        let mut items = Vec::with_capacity(stored.len());
        for StoredScanItem {
            key,
            generation: entry_generation,
            item,
        } in stored
        {
            if entry_generation > generation {
                return Err(DbError::Message(format!(
                    "folder scan entry {key} has generation {entry_generation} newer than \
                     root generation {generation}"
                )));
            }
            validate_scan_item_ownership(&watched_folder_path, &key, &item)?;
            items.push(item);
        }
        snapshots.push(DbFolderScanSnapshot {
            watched_folder_path,
            generation,
            status,
            items,
        });
    }
    Ok(snapshots)
}

pub(super) fn validate_scan_item_ownership(
    watched_folder_path: &str,
    entry_key: &str,
    item: &ScanItem,
) -> Result<(), DbError> {
    if item.persisted_key() != entry_key {
        return Err(DbError::Message(format!(
            "folder scan entry key {entry_key} does not match its item key {}",
            item.persisted_key()
        )));
    }
    let root = Path::new(watched_folder_path);
    let (item_root, item_path) = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        ScanItem::Invalid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        ScanItem::Boundary(boundary) => (
            boundary.key.watched_folder_path.as_str(),
            Path::new(entry_key),
        ),
    };
    if item_root != watched_folder_path || !item_path.starts_with(root) {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} does not belong to watched folder {watched_folder_path}"
        )));
    }
    match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            if !candidate.file_root.starts_with(root) {
                return Err(DbError::Message(format!(
                    "folder scan entry {entry_key} reads files outside its watched folder"
                )));
            }
            for resolved in &candidate.resolved_boundaries {
                validate_decision_key_ownership(watched_folder_path, &resolved.key)?;
            }
            if let Some(key) = &candidate.combine_ancestor_key {
                validate_decision_key_ownership(watched_folder_path, key)?;
            }
        }
        ScanItem::Invalid(candidate) => {
            for resolved in &candidate.resolved_boundaries {
                validate_decision_key_ownership(watched_folder_path, &resolved.key)?;
            }
        }
        ScanItem::Boundary(boundary) => {
            validate_decision_key_ownership(watched_folder_path, &boundary.key)?;
            for row in &boundary.tree_rows {
                validate_decision_key_ownership(watched_folder_path, &row.decision_key)?;
                for ancestor in &row.ancestor_decision_keys {
                    validate_decision_key_ownership(watched_folder_path, ancestor)?;
                }
            }
            if boundary
                .candidate_keys
                .iter()
                .any(|key| !Path::new(key).starts_with(root))
            {
                return Err(DbError::Message(format!(
                    "folder scan boundary {entry_key} contains a candidate outside \
                     its watched folder"
                )));
            }
        }
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
    crate::import::folder_registry::validate_relative_path(&key.relative_folder_path)
        .map_err(|error| DbError::Message(error.to_string()))
}
