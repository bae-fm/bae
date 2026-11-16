//! The watched folders, `watched_import_folders`: which folders import reads,
//! each keyed by its canonical spelling and in the order it was added. No two
//! overlap: every write that watches a folder refuses one that does
//! ([`watch_folder_on`] says how). Every folder a scan, a decision or a skip
//! stores lies under one. Its table is `watched_folders.sql`.

use super::*;
use crate::RelativeFolder;
use std::path::{Path, PathBuf};

mod removal;
pub mod watched_path;
pub use removal::WatchedFoldersRemoved;

/// The watched folders' part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("watched_folders.sql")]);

/// Why a change to the watched folders, or a write about a folder under one,
/// was refused. Nothing of it was written.
#[derive(Debug, thiserror::Error)]
pub enum WatchedFolderRefusal {
    /// The folder lies under no watched folder: the one it was under
    /// stopped being watched.
    #[error("{} is under no watched folder", coven::path_text(.0))]
    Unwatched(PathBuf),
    /// The folder came to be watched while the add was deciding.
    #[error("watched folder {} was added while this add was deciding", coven::path_text(.0))]
    AddedMeanwhile(PathBuf),
    /// The folder holds, or lies in, a watched folder.
    #[error(
        "watched folders cannot overlap: {} conflicts with {}",
        coven::path_text(folder),
        coven::path_text(watched)
    )]
    Overlap { folder: PathBuf, watched: PathBuf },
    /// A removal named no folder.
    #[error("removing watched folders requires at least one")]
    NoneToRemove,
    /// A folder to stop watching is not watched: it stopped being watched
    /// meanwhile.
    #[error("{} is not a watched folder", coven::path_text(.0))]
    NotWatched(PathBuf),
    /// The folder to watch in place of the ones inside it is watched, or
    /// lies in a watched folder.
    #[error(
        "watching {} in place of the folders inside it: {} already watches it",
        coven::path_text(parent),
        coven::path_text(holder)
    )]
    ParentWatched { parent: PathBuf, holder: PathBuf },
    /// The watched folders inside the one to watch in their place are not
    /// the ones the removal names: they changed meanwhile.
    #[error(
        "watching {} in place of the folders inside it: they are {:?}, not {:?}",
        coven::path_text(parent),
        inside.iter().map(|folder| coven::path_text(folder)).collect::<Vec<_>>(),
        named.iter().map(|folder| coven::path_text(folder)).collect::<Vec<_>>()
    )]
    InsideChanged { parent: PathBuf, inside: Vec<PathBuf>, named: Vec<PathBuf> },
}

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
pub(super) fn require_watched(
    sql: &CovenWrite<'_>,
    folder: &Path,
) -> Result<PathBuf, crate::ImportError> {
    watched_folder_among_on(sql, folder.ancestors())?
        .ok_or_else(|| WatchedFolderRefusal::Unwatched(folder.to_path_buf()).into())
}

/// The watched folder among `folders`, each looked up by the watched
/// folders' key. Watched folders never overlap, so the ancestors of one
/// folder name at most one.
pub(super) fn watched_folder_among_on<'a>(
    sql: &impl coven::SqlRead,
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
pub(super) fn watched_roots_on(sql: &impl coven::SqlRead) -> Result<Vec<PathBuf>, DbError> {
    Ok(sql.query("SELECT path FROM watched_import_folders ORDER BY position", [], |row| {
        path_column::get(row, 0)
    })?)
}

/// Every watched folder, in the order they were added. A stored root that
/// is not canonical fails the read.
pub(super) fn watched_folders_on(
    sql: &impl coven::SqlRead,
) -> Result<Vec<crate::WatchedFolder>, DbError> {
    watched_roots_on(sql)?
        .into_iter()
        .map(|root| {
            crate::tables::watched_path::validate_absolute_root(&root)
                .map_err(|_refused| DbError::unreadable("a stored watched folder", &root))?;
            Ok(crate::WatchedFolder::from_path(root))
        })
        .collect()
}

/// Whether `folder`, spelled as the store keys it, is a watched folder, read
/// by the watched folders' key.
fn is_watched_on(sql: &impl coven::SqlRead, folder: &Path) -> Result<bool, DbError> {
    Ok(sql.query_row(
        "SELECT EXISTS (SELECT 1 FROM watched_import_folders WHERE path = ?)",
        [coven::path_bytes(folder)],
        |row| row.get(0),
    )?)
}

/// Whether any folder is watched.
pub(super) fn any_watched_on(sql: &impl coven::SqlRead) -> Result<bool, DbError> {
    Ok(sql
        .query_row("SELECT EXISTS (SELECT 1 FROM watched_import_folders)", [], |row| row.get(0))?)
}

/// A watched folder other than `folder` that overlaps it: the watched folder
/// that holds `folder`, or else the first watched folder inside it, in their
/// order. The read looks up the folders above `folder` by the watched
/// folders' key and finds the folders below it in the range of paths below
/// it.
fn overlapping_watched_folder_on(
    sql: &impl coven::SqlRead,
    folder: &Path,
) -> Result<Option<PathBuf>, DbError> {
    if let Some(above) = watched_folder_among_on(sql, folder.ancestors().skip(1))? {
        return Ok(Some(above));
    }
    Ok(watched_folders_below_on(sql, folder)?.into_iter().next())
}

/// The watched folders inside `folder`, not `folder` itself, in their order:
/// those whose key lies in the range of paths below it.
fn watched_folders_below_on(
    sql: &impl coven::SqlRead,
    folder: &Path,
) -> Result<Vec<PathBuf>, DbError> {
    let (first, past) = crate::tables::watched_path::paths_below(folder);
    Ok(sql.query(
        "SELECT path FROM watched_import_folders \
         WHERE path >= ? AND path < ? AND path <> ? ORDER BY position",
        params![first, past, coven::path_bytes(folder)],
        |row| path_column::get(row, 0),
    )?)
}

/// Refuse `folder` when it holds, or lies in, a watched folder. The error
/// names that watched folder.
fn refuse_overlap_on(sql: &CovenWrite<'_>, folder: &Path) -> Result<(), crate::ImportError> {
    match overlapping_watched_folder_on(sql, folder)? {
        Some(watched) => {
            Err(WatchedFolderRefusal::Overlap { folder: folder.to_path_buf(), watched }.into())
        }
        None => Ok(()),
    }
}

/// Watch `folder`, keyed by its canonical spelling, after every folder
/// watched now. Each caller checks, in the same write, that `folder`
/// overlaps no watched folder: the add with [`refuse_overlap_on`], and the
/// takeover by checking that no watched folder holds `folder` and that the
/// watched folders inside it are the ones the takeover deleted.
pub(super) fn watch_folder_on(sql: &CovenWrite<'_>, folder: &Path) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO watched_import_folders (path, position) \
         SELECT ?, COALESCE(MAX(position) + 1, 0) FROM watched_import_folders",
        [coven::path_bytes(folder)],
    )?;
    Ok(())
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
        let path = path.to_path_buf();
        self.store.read(move |sql| is_watched_on(&sql, &path)).await
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
        let path = path.to_path_buf();
        self.store.read(move |sql| watched_folders_below_on(&sql, &path)).await
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
        self.store
            .write(move |sql| {
                let sql = &sql;
                if is_watched_on(sql, &path)? {
                    return Err(WatchedFolderRefusal::AddedMeanwhile(path).into());
                }
                refuse_overlap_on(sql, &path)?;
                watch_folder_on(sql, &path)?;
                Ok(Some(watched_folders_on(sql)?))
            })
            .await
    }
}

impl ImportTables {
    /// Plant damage for a test: `folder` stored as watched in a spelling
    /// that is not its canonical one, with a separator after its last name.
    /// The add stores only canonical spellings, so no write stores one; a
    /// damaged store holds one, and every read of the watched folders
    /// refuses it.
    #[cfg(test)]
    pub(crate) async fn plant_uncanonical_watched_folder_for_test(
        &self,
        folder: &Path,
    ) -> Result<(), DbError> {
        let mut spelling = coven::path_bytes(folder);
        spelling.push(b'/');
        self.store
            .call(move |sql| {
                sql.execute(
                    "INSERT INTO watched_import_folders (path, position) VALUES (?, 99)",
                    [spelling],
                )?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: inside the write of every folder added to
    /// the library under `directory`, the folder's spelling with a separator
    /// after its last name is stored as watched too, which every read of the
    /// watched folders refuses.
    pub(crate) async fn plant_uncanonical_spelling_in_every_add_for_test(directory: &Path) {
        let directory = directory.to_path_buf();
        bae_util::test_blocking::on_blocking_pool(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "watched_import_folders",
                "CREATE TRIGGER uncanonical_spelling_of_each_added_folder \
                 AFTER INSERT ON watched_import_folders \
                 WHEN NEW.position <> 99 BEGIN \
                 INSERT INTO watched_import_folders (path, position) \
                 VALUES (CAST(CAST(NEW.path AS TEXT) || '/' AS BLOB), 99); END;",
            )
        })
        .await;
    }
}

#[cfg(test)]
#[path = "watched_folders_tests.rs"]
mod tests;
