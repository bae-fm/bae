//! The durable folder-scan tables: `folder_scan_roots` and
//! `folder_scan_entries`. A scan generation is durable before traversal
//! begins; entries are written as they are discovered, each deleting what it
//! supersedes; successful completion prunes entries not seen in that
//! generation in the same transaction that marks the root complete.

use super::import_state::next_folder_scan_generation;
use super::*;
use crate::import::folder_scanner::FolderReleaseDecisionKey;

impl Database {
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
        item: &crate::import::folder_scanner::ScanItem,
    ) -> Result<Option<Vec<String>>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = i64::try_from(generation).map_err(|_| {
            DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
        })?;
        let entry_key = item.persisted_key();
        validate_scan_item_ownership(&watched_folder_path, &entry_key, item)?;
        let encoded = serde_json::to_string(item)
            .map_err(|error| DbError::Message(format!("encoding folder scan item: {error}")))?;
        let item = item.clone();
        // A generation that is no longer the root's writes nothing, and
        // finding that out is a read — the write below opens only once this
        // generation is the one in force. Its statements still carry the
        // generation, so a later one taking over between the two writes
        // nothing rather than writing over it.
        let current: Option<i64> = {
            let watched_folder_path = watched_folder_path.clone();
            self.read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
                        [&watched_folder_path],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await?
        };
        if current != Some(generation) {
            return Ok(None);
        }
        self.call(move |sql| {
            let existing = sql.query(
                "SELECT entry_key, json_type(item, '$.Boundary') IS NOT NULL \
                 FROM folder_scan_entries WHERE watched_folder_path = ?",
                [&watched_folder_path],
                |row| {
                    Ok(crate::import::candidates::StoredEntryKey {
                        key: row.get(0)?,
                        is_boundary: row.get(1)?,
                    })
                },
            )?;
            let removed_keys = crate::import::candidates::superseded_entry_keys(&existing, &item);
            for removed_key in &removed_keys {
                sql.execute(
                    "DELETE FROM folder_scan_entries \
                     WHERE watched_folder_path = ? AND entry_key = ? \
                       AND EXISTS (\
                           SELECT 1 FROM folder_scan_roots \
                           WHERE watched_folder_path = ? AND generation = ?\
                       )",
                    params![
                        watched_folder_path,
                        removed_key,
                        watched_folder_path,
                        generation
                    ],
                )?;
            }
            let changed = sql.execute(
                "INSERT INTO folder_scan_entries \
                     (watched_folder_path, entry_key, generation, item) \
                 SELECT ?, ?, ?, ? \
                 WHERE EXISTS (\
                     SELECT 1 FROM folder_scan_roots \
                     WHERE watched_folder_path = ? AND generation = ?\
                 ) \
                 ON CONFLICT(watched_folder_path, entry_key) DO UPDATE SET \
                     generation = excluded.generation, item = excluded.item",
                params![
                    watched_folder_path,
                    entry_key,
                    generation,
                    encoded,
                    watched_folder_path,
                    generation
                ],
            )?;
            if changed != 1 {
                return Err(DbError::Message(format!(
                    "folder scan item write for {entry_key} under generation {generation} \
                     changed {changed} rows: a newer scan took the root between the check \
                     and the write"
                )));
            }
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
        let generation = i64::try_from(generation).map_err(|_| {
            DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
        })?;
        let error = error.map(str::to_string);
        // A generation that is no longer the root's finishes nothing, and
        // finding that out is a read — the write below opens only once this
        // generation is the one in force. Its statements still carry the
        // generation, so a later one taking over between the two writes
        // nothing rather than finishing the wrong scan.
        let current: Option<i64> = {
            let watched_folder_path = watched_folder_path.clone();
            self.read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
                        [&watched_folder_path],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await?
        };
        if current != Some(generation) {
            return Ok(None);
        }
        self.call(move |sql| {
            if let Some(error) = error {
                sql.execute(
                    "UPDATE folder_scan_roots SET status = 'failed', error = ? \
                     WHERE watched_folder_path = ? AND generation = ?",
                    params![error, watched_folder_path, generation],
                )?;
                return Ok(Some(Vec::new()));
            }
            let pruned = sql.query(
                "SELECT entry_key FROM folder_scan_entries \
                 WHERE watched_folder_path = ? AND generation != ? ORDER BY entry_key",
                params![watched_folder_path, generation],
                |row| row.get::<_, String>(0),
            )?;
            sql.execute(
                "DELETE FROM folder_scan_entries \
                 WHERE watched_folder_path = ? AND generation != ?",
                params![watched_folder_path, generation],
            )?;
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
    ) -> Result<Vec<crate::import::folder_scanner::ScanItem>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| load_folder_scan_items_on(&sql, &watched_folder_path))
            .await
    }

    /// Every stored entry under every watched root.
    pub async fn load_all_folder_scan_items(
        &self,
    ) -> Result<Vec<crate::import::folder_scanner::ScanItem>, DbError> {
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
    ) -> Result<Option<crate::import::folder_scanner::ScanItem>, DbError> {
        let entry_key = entry_key.to_string();
        self.read(move |sql| {
            let rows = sql.query(
                "SELECT watched_folder_path, item FROM folder_scan_entries WHERE entry_key = ?",
                [&entry_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            if rows.len() > 1 {
                return Err(DbError::Message(format!(
                    "folder scan entry {entry_key} is stored under {} roots",
                    rows.len()
                )));
            }
            rows.into_iter()
                .map(|(watched_folder_path, stored)| {
                    decode_scan_entry(&watched_folder_path, &entry_key, &stored)
                })
                .next()
                .transpose()
        })
        .await
    }
}

fn decode_scan_entry(
    watched_folder_path: &str,
    entry_key: &str,
    stored: &str,
) -> Result<crate::import::folder_scanner::ScanItem, DbError> {
    let item: crate::import::folder_scanner::ScanItem =
        serde_json::from_str(stored).map_err(|error| {
            DbError::Message(format!(
                "folder scan entry {entry_key} under {watched_folder_path} is unreadable: {error}"
            ))
        })?;
    validate_scan_item_ownership(watched_folder_path, entry_key, &item)?;
    Ok(item)
}

pub(super) fn load_folder_scan_items_on(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
) -> Result<Vec<crate::import::folder_scanner::ScanItem>, DbError> {
    sql.query(
        "SELECT entry_key, item FROM folder_scan_entries \
         WHERE watched_folder_path = ? ORDER BY entry_key",
        [watched_folder_path],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?
    .into_iter()
    .map(|(entry_key, stored)| decode_scan_entry(watched_folder_path, &entry_key, &stored))
    .collect()
}

pub(super) fn load_folder_scan_snapshots_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<DbFolderScanSnapshot>, DbError> {
    {
        {
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
                    ("failed", Some(error)) => {
                        crate::import::FolderScanStatus::Failed { error }
                    }
                    (status, error) => {
                        return Err(DbError::Message(format!(
                            "folder scan root {watched_folder_path} has invalid status {status:?} and error {error:?}"
                        )))
                    }
                };
                let entries = sql.query(
                    "SELECT entry_key, generation, item FROM folder_scan_entries \
                     WHERE watched_folder_path = ? ORDER BY entry_key",
                    [&watched_folder_path],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )?;
                let mut items = Vec::with_capacity(entries.len());
                for (entry_key, entry_generation, stored) in entries {
                    let entry_generation = u64::try_from(entry_generation).map_err(|_| {
                        DbError::Message(format!(
                            "folder scan entry {entry_key} has a negative generation"
                        ))
                    })?;
                    if entry_generation > generation {
                        return Err(DbError::Message(format!(
                            "folder scan entry {entry_key} has generation {entry_generation} newer than root generation {generation}"
                        )));
                    }
                    let item: crate::import::folder_scanner::ScanItem =
                        serde_json::from_str(&stored).map_err(|error| {
                        DbError::Message(format!(
                            "folder scan entry {entry_key} under {watched_folder_path} is unreadable: {error}"
                        ))
                    })?;
                    validate_scan_item_ownership(&watched_folder_path, &entry_key, &item)?;
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
    }
}

pub(super) fn validate_scan_item_ownership(
    watched_folder_path: &str,
    entry_key: &str,
    item: &crate::import::folder_scanner::ScanItem,
) -> Result<(), DbError> {
    if item.persisted_key() != entry_key {
        return Err(DbError::Message(format!(
            "folder scan entry key {entry_key} does not match its item key {}",
            item.persisted_key()
        )));
    }
    let root = std::path::Path::new(watched_folder_path);
    let (item_root, item_path) = match item {
        crate::import::folder_scanner::ScanItem::Discovered(candidate)
        | crate::import::folder_scanner::ScanItem::Valid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        crate::import::folder_scanner::ScanItem::Invalid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        crate::import::folder_scanner::ScanItem::Boundary(boundary) => (
            boundary.key.watched_folder_path.as_str(),
            std::path::Path::new(entry_key),
        ),
    };
    if item_root != watched_folder_path || !item_path.starts_with(root) {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} does not belong to watched folder {watched_folder_path}"
        )));
    }
    match item {
        crate::import::folder_scanner::ScanItem::Discovered(candidate)
        | crate::import::folder_scanner::ScanItem::Valid(candidate) => {
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
        crate::import::folder_scanner::ScanItem::Invalid(candidate) => {
            for resolved in &candidate.resolved_boundaries {
                validate_decision_key_ownership(watched_folder_path, &resolved.key)?;
            }
        }
        crate::import::folder_scanner::ScanItem::Boundary(_) => {}
    }
    if let crate::import::folder_scanner::ScanItem::Boundary(boundary) = item {
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
            .any(|key| !std::path::Path::new(key).starts_with(root))
        {
            return Err(DbError::Message(format!(
                "folder scan boundary {entry_key} contains a candidate outside its watched folder"
            )));
        }
    }
    Ok(())
}

fn validate_decision_key_ownership(
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
