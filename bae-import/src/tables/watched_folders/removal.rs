use super::*;
use crate::CandidateKey;
use std::path::{Path, PathBuf};

/// What a removal of watched folders changed, as its write left the store.
#[derive(Debug)]
pub struct WatchedFoldersRemoved {
    /// The releases that left the queue.
    pub removed_keys: Vec<CandidateKey>,
    /// The folders still watched, read in the removal's own write.
    pub watched: Vec<crate::WatchedFolder>,
}

impl ImportTables {
    /// Stop watching `roots` in one write, watching `parent` in their place
    /// when given. Returns the keys of the releases that left the queue and
    /// the folders the write leaves watched, or `None` when none of `roots`
    /// is watched and `parent`, if given, is.
    ///
    /// Each root's scan rows go with it. What is known about the folders under
    /// the roots — what was decided, and when each was found — stays when
    /// `parent` takes them over, since the same folders are still watched, and
    /// goes otherwise.
    ///
    /// `parent` must hold exactly `roots` among the watched folders, checked
    /// inside the write.
    pub async fn remove_watched_import_folders(
        &self,
        roots: Vec<PathBuf>,
        parent: Option<PathBuf>,
    ) -> Result<Option<WatchedFoldersRemoved>, DbError> {
        // Spelled the way the add keyed them.
        let roots = roots
            .iter()
            .map(|root| crate::tables::watched_path::canonical_absolute_root(root))
            .collect::<Result<Vec<_>, _>>()?;
        let parent = parent
            .map(|parent| crate::tables::watched_path::canonical_absolute_root(&parent))
            .transpose()?;
        if roots.is_empty() {
            return Err(DbError::Message("removing watched folders requires at least one".into()));
        }
        let watched = self.watched_import_roots().await?;
        if roots.iter().all(|root| !is_among(&watched, root))
            && parent.as_ref().is_none_or(|parent| is_among(&watched, parent))
        {
            return Ok(None);
        }
        self.database
            .call(move |sql| {
                let watched = super::watched_roots_on(sql)?;
                if let Some(root) = roots.iter().find(|root| !is_among(&watched, root)) {
                    return Err(DbError::Message(format!(
                        "{} is not a watched folder",
                        coven::path_text(root)
                    )));
                }
                if let Some(parent) = &parent {
                    watch_in_place_of(sql, &watched, parent, &roots)?;
                }
                let mut removed: Vec<CandidateKey> = Vec::new();
                for root in &roots {
                    let root_bytes = coven::path_bytes(root);
                    removed.extend(sql.query(
                        "SELECT path FROM scan_candidate WHERE watched_folder_path = ?",
                        [&root_bytes],
                        |row| row.get::<_, CandidateKey>(0),
                    )?);
                    let deleted = sql.execute(
                        "DELETE FROM watched_import_folders WHERE path = ?",
                        [&root_bytes],
                    )?;
                    if deleted != 1 {
                        return Err(DbError::Message(format!(
                            "removing watched folder {} changed {deleted} rows; expected one",
                            coven::path_text(root)
                        )));
                    }
                }
                if parent.is_none() {
                    removed.extend(forget_folders_under(sql, &roots)?);
                }
                removed.sort();
                removed.dedup();
                // Every candidate under the folders leaves the scan, and its
                // identification with it.
                for key in &removed {
                    super::import_state::end_file_work_unless_of_on(sql, key, None)?;
                }
                let watched = super::watched_folders_on(sql)?;
                Ok(Some(WatchedFoldersRemoved { removed_keys: removed, watched }))
            })
            .await
    }
}

/// Whether `path` is among the stored `roots`, spelled as they are keyed.
fn is_among(roots: &[PathBuf], path: &Path) -> bool {
    roots.iter().any(|root| root.as_os_str() == path.as_os_str())
}

/// Watch `parent` in place of `roots`, which must be exactly the watched
/// folders inside it, with nothing watching it or a folder holding it.
fn watch_in_place_of(
    sql: &CovenWrite<'_>,
    watched: &[PathBuf],
    parent: &Path,
    roots: &[PathBuf],
) -> Result<(), DbError> {
    if let Some(holder) = watched.iter().find(|root| parent.starts_with(root)) {
        return Err(DbError::Message(format!(
            "watching {} in place of the folders inside it: {} already watches it",
            coven::path_text(parent),
            coven::path_text(holder)
        )));
    }
    fn spelled(mut folders: Vec<&PathBuf>) -> Vec<&PathBuf> {
        folders.sort_by_key(|folder| coven::path_bytes(folder));
        folders
    }
    let inside = spelled(watched.iter().filter(|root| root.starts_with(parent)).collect());
    let expected = spelled(roots.iter().collect());
    if !inside
        .iter()
        .map(|folder| folder.as_os_str())
        .eq(expected.iter().map(|folder| folder.as_os_str()))
    {
        let shown = |folders: &[&PathBuf]| {
            folders.iter().map(|folder| coven::path_text(folder)).collect::<Vec<_>>()
        };
        return Err(DbError::Message(format!(
            "watching {} in place of the folders inside it: they are {:?}, not {:?}",
            coven::path_text(parent),
            shown(&inside),
            shown(&expected)
        )));
    }
    let position: i64 = sql.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM watched_import_folders",
        [],
        |row| row.get(0),
    )?;
    sql.execute(
        "INSERT INTO watched_import_folders (path, position) VALUES (?, ?)",
        params![coven::path_bytes(parent), position],
    )?;
    Ok(())
}

/// Delete what is known about the folders under `roots` and the state of
/// every candidate found only there, each through the owner of its rows.
/// Returns the keys of the grouping releases that went with their groupings.
fn forget_folders_under(
    sql: &CovenWrite<'_>,
    roots: &[PathBuf],
) -> Result<Vec<CandidateKey>, DbError> {
    let mut removed = Vec::new();
    for root in roots {
        let folders = super::FoldersAtOrBelow::of(root);
        crate::tables::skipped_candidates::forget_at_or_below(sql, &folders)?;
        crate::tables::folder_scans::forget_discovery_at_or_below(sql, &folders)?;
        removed.extend(crate::tables::release_groupings::forget_at_or_below(sql, &folders)?);
        crate::tables::import_state::forget_at_or_below(sql, &folders)?;
    }
    Ok(removed)
}

#[cfg(test)]
#[path = "removal_tests.rs"]
mod tests;
