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
use std::path::{Path, PathBuf};

pub(super) use read::{
    load_candidate_items, load_item_by_key, load_resolved_boundaries, stored_entries,
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

/// What one scan item's write did.
///
/// The distinction the import list lives on: a pass that finds a folder exactly
/// as it left it has nothing to tell anyone, and saying so is what keeps a
/// timer-driven re-read of a watched folder free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanItemWrite {
    /// The stored row already said exactly this. It kept its place and took
    /// this generation's stamp, so the completion prune keeps it too.
    Unchanged,
    /// The row was written, displacing the keys named here.
    Stored { superseded_keys: Vec<String> },
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
            Self::Stored { superseded_keys } => superseded_keys,
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

fn generation_column(generation: u64) -> Result<i64, DbError> {
    i64::try_from(generation).map_err(|_| {
        DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
    })
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
                || audio_files.iter().zip(&snapshot.files).any(
                    |((relative_path, size), fact)| {
                        relative_path != &fact.observation.relative_path
                            || u64::try_from(*size).ok() != Some(fact.observation.size)
                    },
                )
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

            write::replace_candidate_file_tag_snapshot(
                sql,
                &watched_folder_path,
                &candidate_path,
                &snapshot,
            )?;
            Ok(true)
        })
        .await
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
    ) -> Result<Option<ScanItemWrite>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = generation_column(generation)?;
        let Some(entry_key) = item.persisted_key() else {
            return Err(DbError::Message(
                "a folder reading is stored as a decision, not as a scan entry".to_string(),
            ));
        };
        validate_scan_item_ownership(&watched_folder_path, &entry_key, item)?;
        let item = item.clone();
        if self.current_scan_generation(&watched_folder_path).await? != Some(generation) {
            return Ok(None);
        }
        self.call(move |sql| {
            ensure_generation(sql, &watched_folder_path, generation)?;
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
            // removed by the boundary that hides it, which supersedes by key
            // and does not care which kind the row was.
            if matches!(item, ScanItem::Discovered(_))
                && read::candidate_is_valid(sql, &watched_folder_path, &entry_key)?
            {
                write::touch_candidate(sql, &watched_folder_path, &entry_key, generation)?;
                return Ok(Some(ScanItemWrite::Unchanged));
            }
            // A walk of a folder nobody has touched produces exactly the items
            // already stored for it. Rewriting one of those would mean a
            // transaction, an announcement, and every reader of the import list
            // rebuilding it — per row, per pass, forever, over a folder that did
            // not change. So the row keeps its place and takes only this
            // generation's stamp, which is all the completion prune asks of it.
            if load_scan_item_on(sql, &entry_key)?.as_ref() == Some(&item) {
                write::touch_candidate(sql, &watched_folder_path, &entry_key, generation)?;
                return Ok(Some(ScanItemWrite::Unchanged));
            }
            let stored = stored_entries(sql, &watched_folder_path)?;
            let keys: Vec<StoredEntryKey> = stored
                .iter()
                .map(|(key, entry)| StoredEntryKey {
                    key: key.clone(),
                    covers_whole_folder: matches!(
                        entry,
                        StoredEntry::Candidate {
                            whole_folder: true,
                            ..
                        }
                    ),
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
            Ok(Some(ScanItemWrite::Stored {
                superseded_keys: removed_keys,
            }))
        })
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

    #[cfg(test)]
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

#[cfg(test)]
fn load_folder_scan_snapshots_on(
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
        let items = load_folder_scan_items_on(sql, &watched_folder_path)?;
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
    if item.persisted_key().as_deref() != Some(entry_key) {
        return Err(DbError::Message(format!(
            "folder scan entry key {entry_key} does not match its item key {:?}",
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
        ScanItem::Decided { key, .. } => (key.watched_folder_path.as_str(), Path::new(entry_key)),
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
        ScanItem::Decided { key, .. } => {
            validate_decision_key_ownership(watched_folder_path, key)?;
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
