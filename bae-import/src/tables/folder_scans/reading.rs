//! Reading one folder of a watched root again, stored in the one write that
//! also stores the decision that changed how it reads, so no reader sees the
//! folder with neither reading.

use super::*;
use crate::folder_scanner::{FolderReading, FolderReleaseDecisionKey};
use std::ffi::OsString;
use std::path::Component;

/// Why reading one folder of a watched folder again was refused. Nothing of
/// it was written.
#[derive(Debug, thiserror::Error)]
pub enum FolderRereadRefusal {
    /// The watched folder holds no reading to change: it was never read, or
    /// it was removed.
    #[error("{} has not been read, so none of its folders can be read again", coven::path_text(.0))]
    NeverRead(PathBuf),
    /// The watched folder's entries were written after the reading of the
    /// folder began.
    #[error(
        "{} changed while it was being read again: the root's entries were written after \
         the reading began",
        coven::path_text(.0)
    )]
    RootMoved(PathBuf),
    /// The reading names no one folder directly under its watched folder.
    #[error(
        "{} is not a folder directly under {}",
        coven::path_text(folder),
        coven::path_text(root)
    )]
    NotDirectlyUnder { folder: PathBuf, root: PathBuf },
    /// A decision the reading carries is about a folder outside the one read.
    #[error("folder release decision for {decision} is outside {}", coven::path_text(folder))]
    DecisionOutside { decision: String, folder: PathBuf },
    /// An entry the reading yields lies outside the folder read.
    #[error(
        "folder scan entry at {} is outside {}",
        coven::path_text(entry),
        coven::path_text(folder)
    )]
    EntryOutside { entry: PathBuf, folder: PathBuf },
}

/// Where the root stood when one folder's reading was taken, and the
/// generation that reading is stored under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FolderReadingStamp {
    /// The root's generation when the reading began; the commit refuses a
    /// root that has moved past it.
    pub(crate) read_under: u64,
    /// The generation this reading's entries, and then the root, stand at.
    pub(crate) generation: u64,
}

/// One folder's new reading, ready to store.
pub(crate) struct FolderReadingCommit {
    pub(crate) watched_folder_path: PathBuf,
    /// The name of the folder directly under the root that was read again.
    pub(crate) folder: OsString,
    pub(crate) stamp: FolderReadingStamp,
    /// The person's answer, when that is why the folder was read again.
    pub(crate) decision: Option<(FolderReleaseDecisionKey, FolderReading)>,
    /// How the walk read folders nothing was stored for.
    pub(crate) scanned_decisions: Vec<(FolderReleaseDecisionKey, FolderReading)>,
    /// Every entry the folder yields, in the order the walk yielded them.
    pub(crate) items: Vec<FolderEntryToWrite>,
    /// Every directory in the folder with its mtime, or `None` when one could
    /// not be read, which clears the root's record.
    pub(crate) directories: Option<Vec<(PathBuf, i64)>>,
}

/// What storing one folder's reading did.
pub(crate) struct FolderReadingWrite {
    /// Each entry, with what its write did, in the order they were written.
    pub(crate) writes: Vec<(crate::folder_scanner::FolderEntry, ScanItemWrite)>,
    /// Entries under the folder the new reading no longer yields.
    pub(crate) pruned: Vec<CandidateKey>,
    /// Releases groupings rebuilt without the pruned entries.
    pub(crate) regrouped: crate::tables::release_groupings::GroupingChanges,
}

impl ImportTables {
    /// Begin reading one folder of `watched_folder_path` again: the root's
    /// generation now, and a fresh one to store the reading under. A root never
    /// read has no reading to change.
    pub(crate) async fn begin_folder_reading(
        &self,
        watched_folder_path: &Path,
    ) -> Result<FolderReadingStamp, crate::ImportError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        self.store
            .write(move |sql| {
                let sql = &sql;
                let Some(read_under) = root_generation_on(sql, &watched_folder_path)? else {
                    return Err(FolderRereadRefusal::NeverRead(watched_folder_path).into());
                };
                let generation = next_folder_scan_generation(sql)?;
                Ok(FolderReadingStamp {
                    read_under: to_u64(read_under, "folder_scan_roots.generation")?,
                    generation: to_u64(generation, "a folder scan generation")?,
                })
            })
            .await
    }

    /// Store one folder's new reading in one transaction: the decision, every
    /// entry the folder now yields, and the removal of the ones it no longer
    /// yields. Writes nothing when the root's entries were written since the
    /// reading began.
    pub(crate) async fn commit_folder_reading(
        &self,
        commit: FolderReadingCommit,
    ) -> Result<FolderReadingWrite, crate::ImportError> {
        let FolderReadingCommit {
            watched_folder_path,
            folder,
            stamp,
            decision,
            scanned_decisions,
            items,
            directories,
        } = commit;
        let mut names = Path::new(&folder).components();
        let one_name = matches!(
            (names.next(), names.next()),
            (Some(Component::Normal(name)), None) if name == folder.as_os_str()
        );
        if !one_name {
            return Err(FolderRereadRefusal::NotDirectlyUnder {
                folder: PathBuf::from(folder),
                root: watched_folder_path,
            }
            .into());
        }
        let folder_path = watched_folder_path.join(&folder);
        for (key, _) in decision.iter().chain(scanned_decisions.iter()) {
            validate_decision_key_ownership(&watched_folder_path, key)?;
            if !crate::tables::watched_path::folder_below(
                &key.watched_folder_path,
                &key.relative_folder,
            )
            .starts_with(&folder_path)
            {
                return Err(FolderRereadRefusal::DecisionOutside {
                    decision: key.relative_folder.text().to_string(),
                    folder: folder_path,
                }
                .into());
            }
        }
        for item in &items {
            let coverage = match item {
                FolderEntryToWrite::Release(to_write) => to_write.item.coverage(),
                FolderEntryToWrite::Sidecar(sidecar) => sidecar.coverage(),
            };
            if !coverage.folder.starts_with(&folder_path) {
                return Err(FolderRereadRefusal::EntryOutside {
                    entry: coverage.folder,
                    folder: folder_path,
                }
                .into());
            }
        }
        let read_under = generation_column(stamp.read_under)?;
        let generation = generation_column(stamp.generation)?;
        let observed_at = self.store.now_utc().timestamp_millis();
        self.store
            .write(move |sql| {
                let sql = &sql;
                let root = coven::path_bytes(&watched_folder_path);
                if root_generation_on(sql, &watched_folder_path)? != Some(read_under) {
                    return Err(FolderRereadRefusal::RootMoved(folder_path).into());
                }
                for (key, reading) in decision.iter().chain(scanned_decisions.iter()) {
                    store_folder_reading(
                        sql,
                        key,
                        reading.decision,
                        reading.author,
                        &reading.grouping,
                    )?;
                }
                sql.execute(
                    "UPDATE folder_scan_roots SET generation = ? WHERE watched_folder_path = ?",
                    params![generation, root],
                )?;
                let finding = match decision {
                    Some(_) => Finding::Decision,
                    None => Finding::FirstRead,
                };
                let mut writes = Vec::with_capacity(items.len());
                for item in items {
                    let write = write_scan_item(
                        sql,
                        &watched_folder_path,
                        generation,
                        &item,
                        observed_at,
                        finding,
                    )?;
                    writes.push((item.into_entry(), write));
                }
                let folder_bytes = coven::path_bytes(&folder_path);
                let (below_first, below_past) =
                    crate::tables::watched_path::paths_below(&folder_path);
                let pruned: Vec<CandidateKey> = sql.query(
                    "SELECT path FROM scan_candidate \
                 WHERE watched_folder_path = :root AND generation != :generation \
                   AND source_kind = 'folder' \
                   AND (folder = :folder \
                        OR (folder >= :below_first AND folder < :below_past)) \
                 ORDER BY path",
                    rusqlite::named_params! {
                        ":root": root,
                        ":generation": generation,
                        ":folder": folder_bytes,
                        ":below_first": below_first,
                        ":below_past": below_past,
                    },
                    |row| row.get::<_, CandidateKey>(0),
                )?;
                for key in &pruned {
                    delete_entry(sql, &watched_folder_path, key)?;
                }
                let pruned_sidecars = super::sidecar::prune_sidecars(
                    sql,
                    &watched_folder_path,
                    generation,
                    Some(&folder_path),
                )?;
                match &directories {
                    Some(directories) => {
                        sql.execute(
                            "DELETE FROM folder_scan_directory \
                         WHERE watched_folder_path = :root \
                           AND (path = :folder \
                                OR (path >= :below_first AND path < :below_past))",
                            rusqlite::named_params! {
                                ":root": root,
                                ":folder": folder_bytes,
                                ":below_first": below_first,
                                ":below_past": below_past,
                            },
                        )?;
                        for (path, modified_at) in directories {
                            // The walk of the folder records only the
                            // directories below it.
                            assert!(
                                path.starts_with(&folder_path),
                                "recorded directory {} is outside {}",
                                coven::path_text(path),
                                coven::path_text(&folder_path)
                            );
                            sql.execute(
                                "INSERT INTO folder_scan_directory \
                                 (watched_folder_path, path, modified_at) VALUES (?, ?, ?)",
                                params![root, coven::path_bytes(path), modified_at],
                            )?;
                        }
                    }
                    None => {
                        sql.execute(
                            "DELETE FROM folder_scan_directory WHERE watched_folder_path = ?",
                            [&root],
                        )?;
                    }
                }
                let regrouped = crate::tables::release_groupings::rebuild_groupings(
                    sql,
                    &pruned,
                    &pruned_sidecars,
                )?;
                Ok(FolderReadingWrite { writes, pruned, regrouped })
            })
            .await
    }
}

#[cfg(test)]
#[path = "reading_tests.rs"]
mod tests;
