//! Reading one folder of a watched root again, as one write.
//!
//! A folder that changes how it reads — the person keeps its discs as
//! separate releases, or combines them into one — trades the candidates of
//! the old reading for the candidates of the new one. Both sets are stored in
//! the transaction that stores the decision, so no reader ever sees the folder
//! with neither: the reading is taken first, off the store, and this commits it
//! whole or not at all.

use super::*;
use crate::import::folder_scanner::{
    FolderReleaseDecision, FolderReleaseDecisionAuthor, FolderReleaseDecisionKey,
};

/// Where the root stood when one folder's reading was taken, and the
/// generation that reading is stored under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FolderReadingStamp {
    /// The root's generation when the reading began. The commit refuses a
    /// root that has moved past it: something else wrote the root's entries
    /// after this reading looked at the folder.
    pub(crate) read_under: u64,
    /// The generation every entry of this reading is stamped with, and the
    /// one the root stands at once it is stored.
    pub(crate) generation: u64,
}

/// One folder's new reading, ready to store.
pub(crate) struct FolderReadingCommit {
    pub(crate) watched_folder_path: String,
    /// The folder directly under the root that was read again, `/`-free —
    /// everything below it is this reading's, and nothing outside it is.
    pub(crate) folder: String,
    pub(crate) stamp: FolderReadingStamp,
    /// The person's answer for the folder whose reading changed, when that is
    /// why the folder was read again.
    pub(crate) decision: Option<(FolderReleaseDecisionKey, FolderReleaseDecision)>,
    /// How the walk read folders nothing was stored for. Never replaces an
    /// answer the person gave.
    pub(crate) scanned_decisions: Vec<(FolderReleaseDecisionKey, FolderReleaseDecision)>,
    /// Every entry the folder yields, in the order the walk yielded them.
    pub(crate) items: Vec<ScanItemToWrite>,
    /// Every directory in the folder with the mtime it had, replacing what
    /// was recorded under it — or `None` when one could not be read, which
    /// clears the root's record so the next cheap check walks.
    pub(crate) directories: Option<Vec<(String, i64)>>,
}

/// What storing one folder's reading did.
pub(crate) struct FolderReadingWrite {
    /// Each entry, with what its write did, in the order they were written.
    pub(crate) writes: Vec<(ScanItem, ScanItemWrite)>,
    /// Entries under the folder the new reading no longer yields.
    pub(crate) pruned: Vec<String>,
}

impl Database {
    /// Begin reading one folder of `watched_folder_path` again: the root's
    /// generation now, and a fresh one for the reading to be stored under.
    ///
    /// A root with no scan generation has never been read, so it has no
    /// reading of any folder to change.
    pub(crate) async fn begin_folder_reading(
        &self,
        watched_folder_path: &str,
    ) -> Result<FolderReadingStamp, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.call(move |sql| {
            let read_under: Option<i64> = sql
                .query_row(
                    "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
                    [&watched_folder_path],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(read_under) = read_under else {
                return Err(DbError::Message(format!(
                    "{watched_folder_path} has not been read, so none of its folders can be \
                     read again"
                )));
            };
            let generation = next_folder_scan_generation(sql)?;
            Ok(FolderReadingStamp {
                read_under: columns::to_u64(read_under, "a folder scan root's generation")?,
                generation: columns::to_u64(generation, "a folder scan generation")?,
            })
        })
        .await
    }

    /// Store one folder's new reading in one transaction: the decision that
    /// changed it, every entry the folder now yields, and the removal of every
    /// entry under it that the new reading does not yield.
    ///
    /// Fails, writing nothing, when the root's entries were written since the
    /// reading began — the reading describes a store that is no longer there.
    pub(crate) async fn commit_folder_reading(
        &self,
        commit: FolderReadingCommit,
    ) -> Result<FolderReadingWrite, DbError> {
        let FolderReadingCommit {
            watched_folder_path,
            folder,
            stamp,
            decision,
            scanned_decisions,
            items,
            directories,
        } = commit;
        crate::import::watched_folder::validate_relative_path(&folder)?;
        if folder.contains('/') {
            return Err(DbError::Message(format!(
                "{folder} is not a folder directly under {watched_folder_path}"
            )));
        }
        let folder_path = Path::new(&watched_folder_path).join(&folder);
        for (key, _) in decision.iter().chain(scanned_decisions.iter()) {
            validate_decision_key_ownership(&watched_folder_path, key)?;
            if !Path::new(&key.watched_folder_path)
                .join(&key.relative_folder_path)
                .starts_with(&folder_path)
            {
                return Err(DbError::Message(format!(
                    "folder release decision for {} is outside {folder}",
                    key.relative_folder_path
                )));
            }
        }
        for item in &items {
            let key = item.item.persisted_key().ok_or_else(|| {
                DbError::Message(
                    "a folder reading is stored as a decision, not as a scan entry".to_string(),
                )
            })?;
            if !Path::new(&key).starts_with(&folder_path) {
                return Err(DbError::Message(format!(
                    "folder scan entry {key} is outside {folder}"
                )));
            }
        }
        let read_under = generation_column(stamp.read_under)?;
        let generation = generation_column(stamp.generation)?;
        let observed_at = self.inner.clock.now().timestamp_millis();
        self.call(move |sql| {
            let current: Option<i64> = sql
                .query_row(
                    "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
                    [&watched_folder_path],
                    |row| row.get(0),
                )
                .optional()?;
            if current != Some(read_under) {
                return Err(DbError::Message(format!(
                    "{} changed while it was being read again: the root's entries were \
                     written after the reading began",
                    folder_path.display()
                )));
            }
            if let Some((key, decision)) = &decision {
                store_folder_release_decision(
                    sql,
                    key,
                    *decision,
                    FolderReleaseDecisionAuthor::User,
                )?;
            }
            for (key, decision) in &scanned_decisions {
                store_folder_release_decision(
                    sql,
                    key,
                    *decision,
                    FolderReleaseDecisionAuthor::Heuristic,
                )?;
            }
            sql.execute(
                "UPDATE folder_scan_roots SET generation = ? WHERE watched_folder_path = ?",
                params![generation, watched_folder_path],
            )?;
            let mut writes = Vec::with_capacity(items.len());
            for item in items {
                let write =
                    write_scan_item(sql, &watched_folder_path, generation, &item, observed_at)?;
                writes.push((item.item, write));
            }
            let mut pruned: Vec<String> = sql
                .query(
                    "SELECT path FROM scan_candidate \
                     WHERE watched_folder_path = ? AND generation != ? \
                       AND source_kind = 'folder'",
                    params![watched_folder_path, generation],
                    |row| row.get::<_, String>(0),
                )?
                .into_iter()
                .filter(|path| Path::new(path).starts_with(&folder_path))
                .collect();
            pruned.sort();
            for path in &pruned {
                delete_entry(
                    sql,
                    &watched_folder_path,
                    &StoredEntry::Candidate {
                        path: path.clone(),
                        whole_folder: false,
                    },
                )?;
            }
            let recorded: Vec<String> = sql.query(
                "SELECT path FROM folder_scan_directory WHERE watched_folder_path = ?",
                [&watched_folder_path],
                |row| row.get::<_, String>(0),
            )?;
            match &directories {
                Some(directories) => {
                    for path in recorded
                        .iter()
                        .filter(|path| Path::new(path).starts_with(&folder_path))
                    {
                        sql.execute(
                            "DELETE FROM folder_scan_directory \
                             WHERE watched_folder_path = ? AND path = ?",
                            params![watched_folder_path, path],
                        )?;
                    }
                    for (path, modified_at) in directories {
                        if !Path::new(path).starts_with(&folder_path) {
                            return Err(DbError::Message(format!(
                                "recorded directory {path} is outside {}",
                                folder_path.display()
                            )));
                        }
                        sql.execute(
                            "INSERT INTO folder_scan_directory \
                                 (watched_folder_path, path, modified_at) VALUES (?, ?, ?)",
                            params![watched_folder_path, path, modified_at],
                        )?;
                    }
                }
                None => {
                    sql.execute(
                        "DELETE FROM folder_scan_directory WHERE watched_folder_path = ?",
                        [&watched_folder_path],
                    )?;
                }
            }
            Ok(FolderReadingWrite { writes, pruned })
        })
        .await
    }
}
