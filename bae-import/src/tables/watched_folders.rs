//! The watched folders, `watched_import_folders`: which folders import reads,
//! each keyed by its canonical spelling and in the order it was added. No two
//! overlap. Every folder a scan, a decision or a skip stores lies under one.
//! Its table is `watched_folders.sql`.

use super::*;
use crate::RelativeFolder;
use std::path::{Path, PathBuf};

mod removal;
pub mod watched_path;
pub use removal::WatchedFoldersRemoved;

/// The watched folders' part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("watched_folders.sql")]);

/// The folders at or below one folder, as a condition on a stored folder
/// column: the range each owner of rows kept by folder forgets when the
/// folder stops being watched.
pub(crate) struct FoldersAtOrBelow {
    folder: Vec<u8>,
    below_first: Vec<u8>,
    below_past: Vec<u8>,
}

impl FoldersAtOrBelow {
    pub(crate) fn of(folder: &Path) -> Self {
        let (below_first, below_past) = crate::tables::watched_path::paths_below(folder);
        Self { folder: coven::path_bytes(folder), below_first, below_past }
    }

    /// `column` names one of the folders, given [`Self::params`]. Read on the
    /// column's index, as a range.
    pub(crate) fn condition(column: &str) -> String {
        format!("({column} = :root OR ({column} >= :below_first AND {column} < :below_past))")
    }

    pub(crate) fn params(&self) -> [(&'static str, &dyn rusqlite::ToSql); 3] {
        [
            (":root", &self.folder),
            (":below_first", &self.below_first),
            (":below_past", &self.below_past),
        ]
    }
}

/// `folder`, a stored folder under the watched folder `root`, as the folder
/// below it.
pub(super) fn relative_below(root: &Path, folder: &Path) -> Result<RelativeFolder, DbError> {
    crate::tables::watched_path::candidate_relative_path(root, folder)
        .map_err(|_refused| DbError::unreadable("a stored folder below its watched folder", folder))
}

/// The watched folder covering `folder`. A decision about a folder is deleted
/// with the watched folder covering it, so one stored outside every watched
/// folder would never be deleted.
pub(super) fn require_watched(sql: &CovenWrite<'_>, folder: &Path) -> Result<PathBuf, DbError> {
    watched_folder_among_on(sql, folder.ancestors())?.ok_or_else(|| {
        DbError::Message(format!("{} is under no watched folder", coven::path_text(folder)))
    })
}

/// The watched folder among `folders`, each looked up by the watched
/// folders' key. Watched folders never overlap, so the ancestors of one
/// folder name at most one.
pub(super) fn watched_folder_among_on<'a>(
    sql: &(impl QueryOne + QueryRows),
    folders: impl Iterator<Item = &'a Path>,
) -> Result<Option<PathBuf>, DbError> {
    Ok(sql
        .query_row(
            "SELECT path FROM watched_import_folders \
             WHERE path IN (SELECT unhex(value) FROM json_each(?))",
            [path_column::hex_list(folders)],
            |row| path_column::get(row, 0),
        )
        .optional()?)
}

/// Every watched root, as stored, in the order they were added.
pub(super) fn watched_roots_on(sql: &impl QueryRows) -> Result<Vec<PathBuf>, DbError> {
    Ok(sql.query("SELECT path FROM watched_import_folders ORDER BY position", [], |row| {
        path_column::get(row, 0)
    })?)
}

/// Every watched folder, in the order they were added. A stored root that
/// is not canonical, or overlaps another, fails the read.
pub(super) fn watched_folders_on(
    sql: &impl QueryRows,
) -> Result<Vec<crate::WatchedFolder>, DbError> {
    watched_folders_of(watched_roots_on(sql)?)
}

/// The watched folders `roots`, as stored in the order they were added. A
/// root that is not canonical, or overlaps another, fails the read.
fn watched_folders_of(roots: Vec<PathBuf>) -> Result<Vec<crate::WatchedFolder>, DbError> {
    for (index, root) in roots.iter().enumerate() {
        crate::tables::watched_path::validate_absolute_root(root)
            .map_err(|_refused| DbError::unreadable("a stored watched folder", root))?;
        if let Some(conflict) = roots[index + 1..]
            .iter()
            .find(|other| crate::watched_folder::paths_overlap(root, other))
        {
            return Err(DbError::Message(format!(
                "watched folders cannot overlap: {} conflicts with {}",
                coven::path_text(root),
                coven::path_text(conflict)
            )));
        }
    }
    Ok(roots.into_iter().map(crate::WatchedFolder::from_path).collect())
}

impl ImportTables {
    /// Every watched folder, in the order they were added. A stored root that
    /// is not canonical, or overlaps another, fails the read.
    pub async fn load_watched_import_folders(&self) -> Result<Vec<crate::WatchedFolder>, DbError> {
        self.store.read(move |sql| watched_folders_on(&sql)).await
    }

    /// Whether `path`, spelled as the store keys it, is a watched folder,
    /// read by the watched folders' key.
    pub async fn is_watched_import_folder(&self, path: &Path) -> Result<bool, DbError> {
        let path = coven::path_bytes(path);
        self.store
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM watched_import_folders WHERE path = ?)",
                    [path],
                    |row| row.get(0),
                )?)
            })
            .await
    }

    /// The watched folder that holds `path` inside it, not `path` itself.
    pub async fn watched_import_folder_above(
        &self,
        path: &Path,
    ) -> Result<Option<PathBuf>, DbError> {
        let path = path.to_path_buf();
        self.store.read(move |sql| watched_folder_among_on(&sql, path.ancestors().skip(1))).await
    }

    /// The watched folders inside `path`, not `path` itself, in their order:
    /// those whose key lies in the range of paths below it.
    pub async fn watched_import_folders_below(&self, path: &Path) -> Result<Vec<PathBuf>, DbError> {
        let (first, past) = crate::tables::watched_path::paths_below(path);
        let path = coven::path_bytes(path);
        self.store
            .read(move |sql| {
                Ok(sql.query(
                    "SELECT path FROM watched_import_folders \
                 WHERE path >= ? AND path < ? AND path <> ? ORDER BY position",
                    params![first, past, path],
                    |row| path_column::get(row, 0),
                )?)
            })
            .await
    }

    /// Every watched root, in the order they were added.
    async fn watched_import_roots(&self) -> Result<Vec<PathBuf>, DbError> {
        self.store.read(move |sql| watched_roots_on(&sql)).await
    }

    /// Watch the folder `path` names, keyed by its canonical spelling, and
    /// return the folders watched after the add, read inside its write;
    /// `None` when it is already watched.
    ///
    /// The overlap check runs inside the inserting write, so two overlapping
    /// folders added at once cannot both pass. Already-watched is answered by
    /// a read first, since the store refuses a write that changes nothing; a
    /// folder watched in between fails the write.
    pub async fn add_watched_import_folder(
        &self,
        path: &Path,
    ) -> Result<Option<Vec<crate::WatchedFolder>>, crate::ImportError> {
        let path = crate::tables::watched_path::canonical_absolute_root(path)?;
        if self.is_watched_import_folder(&path).await? {
            return Ok(None);
        }
        Ok(self
            .store
            .call(move |sql| {
                let roots = watched_roots_on(sql)?;
                if roots.iter().any(|root| root.as_os_str() == path.as_os_str()) {
                    return Err(DbError::Message(format!(
                        "watched folder {} was added while this add was deciding",
                        coven::path_text(&path)
                    )));
                }
                if let Some(conflict) =
                    roots.iter().find(|root| crate::watched_folder::paths_overlap(&path, root))
                {
                    return Err(DbError::Message(format!(
                        "watched folders cannot overlap: {} conflicts with {}",
                        coven::path_text(&path),
                        coven::path_text(conflict)
                    )));
                }
                let position: i64 = sql.query_row(
                    "SELECT COALESCE(MAX(position) + 1, 0) FROM watched_import_folders",
                    [],
                    |row| row.get(0),
                )?;
                sql.execute(
                    "INSERT INTO watched_import_folders (path, position) VALUES (?, ?)",
                    params![coven::path_bytes(&path), position],
                )?;
                Ok(Some(watched_folders_on(sql)?))
            })
            .await?)
    }
}

impl ImportTables {
    /// Plant damage for a test: `inner`, a folder inside a watched one,
    /// stored as watched too. The add refuses a folder that overlaps another,
    /// so no write stores one; a damaged store holds one, and every read of
    /// the watched folders checks for it.
    #[cfg(test)]
    pub(crate) async fn plant_overlapping_watched_folder_for_test(
        &self,
        inner: &Path,
    ) -> Result<(), DbError> {
        let inner = coven::path_bytes(inner);
        self.store
            .call(move |sql| {
                sql.execute(
                    "INSERT INTO watched_import_folders (path, position) VALUES (?, 99)",
                    [inner],
                )?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: inside the write of every folder added to
    /// the library under `directory`, a folder inside it is stored as
    /// watched too, which every read of the watched folders refuses.
    pub(crate) async fn plant_overlap_in_every_add_for_test(directory: &Path) {
        let directory = directory.to_path_buf();
        tokio::task::spawn_blocking(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "watched_import_folders",
                "CREATE TRIGGER overlap_each_added_folder AFTER INSERT ON watched_import_folders \
                 WHEN NEW.position <> 99 BEGIN \
                 INSERT INTO watched_import_folders (path, position) \
                 VALUES (CAST(CAST(NEW.path AS TEXT) || '/inner' AS BLOB), 99); END;",
            )
        })
        .await
        .unwrap();
    }
}

#[cfg(test)]
#[path = "watched_folders_tests.rs"]
mod tests;
