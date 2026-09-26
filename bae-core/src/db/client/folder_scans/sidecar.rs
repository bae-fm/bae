//! A folder's sidecar files as the scan stores them: the files under a folder
//! that no release read there owns (see
//! [`FolderSidecar`](crate::import::folder_scanner::FolderSidecar)).
//!
//! A sidecar stands on the same terms as a scan entry. It is written under the
//! scan generation that read it and pruned with the entries that generation
//! did not see. Its files and a release's are never both stored: a sidecar
//! replaces any entry that reads its folder's own files, and an entry that
//! reads them replaces the sidecar. Whatever changes one rebuilds the grouping
//! that reads it.

use super::columns::*;
use super::*;
use crate::import::folder_scanner::{
    CandidateFile, Coverage, FileRole, FolderSidecar, SidecarFiles, ScannedFile,
};

/// The sidecar stored for `folder`, whichever root it is under.
pub(crate) fn load_sidecar(
    sql: &(impl QueryOne + QueryRows),
    folder: &Path,
) -> Result<Option<FolderSidecar>, DbError> {
    let folder_text = folder.to_string_lossy();
    let rows = sql.query(
        "SELECT watched_folder_path, invalid_reason, invalid_reason_path FROM scan_sidecar \
         WHERE folder = ?",
        [&folder_text],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )?;
    if rows.len() > 1 {
        return Err(DbError::Message(format!(
            "{} has a sidecar stored under {} roots",
            folder.display(),
            rows.len()
        )));
    }
    let Some((watched_folder_path, reason, reason_path)) = rows.into_iter().next() else {
        return Ok(None);
    };
    let files = match reason {
        Some(reason) => SidecarFiles::Invalid(invalid_reason_of(&reason, reason_path)?),
        None => SidecarFiles::Valid(
            sql.query(
                "SELECT relative_path, absolute_path, size, modified_at_ns, role \
                 FROM scan_sidecar_file WHERE watched_folder_path = ? AND folder = ? \
                 ORDER BY position",
                params![watched_folder_path, folder_text],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )?
            .into_iter()
            .map(|(relative_path, absolute_path, size, modified_at_ns, role)| {
                Ok(CandidateFile {
                    file: ScannedFile::new(
                        PathBuf::from(absolute_path),
                        relative_path,
                        to_u64(size, "a sidecar file's size")?,
                        modified_at_ns,
                    ),
                    role: match role.as_str() {
                        "artwork" => FileRole::Artwork,
                        "document" => FileRole::Document,
                        "other" => FileRole::Other,
                        other => return Err(unreadable("scan_sidecar_file.role", other)),
                    },
                    proposed_audio: false,
                })
            })
            .collect::<Result<_, DbError>>()?,
        ),
    };
    Ok(Some(FolderSidecar {
        watched_folder_path,
        folder: folder.to_path_buf(),
        files,
    }))
}

/// Store `sidecar` under `generation`, replacing the entries that read its
/// folder's own files, and rebuild whatever grouping reads it or took in a
/// replaced entry. A sidecar stored exactly so only takes the generation.
pub(super) fn write_sidecar(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    sidecar: &FolderSidecar,
    observed_at: i64,
) -> Result<ScanItemWrite, DbError> {
    let root = Path::new(watched_folder_path);
    if sidecar.watched_folder_path != watched_folder_path
        || sidecar.folder == root
        || !sidecar.folder.starts_with(root)
    {
        return Err(DbError::Message(format!(
            "the sidecar of {} does not belong below watched folder {watched_folder_path}",
            sidecar.folder.display()
        )));
    }
    let folder_text = sidecar.folder.to_string_lossy().into_owned();
    if load_sidecar(sql, &sidecar.folder)?.as_ref() == Some(sidecar) {
        sql.execute(
            "UPDATE scan_sidecar SET generation = ? WHERE watched_folder_path = ? AND folder = ?",
            params![generation, watched_folder_path, folder_text],
        )?;
        return Ok(ScanItemWrite::Unchanged);
    }
    let superseded_keys = superseded_keys(
        &stored_entries(sql, watched_folder_path)?,
        &crate::import::folder_scanner::ScanItem::Sidecar(sidecar.clone()),
    );
    for key in &superseded_keys {
        delete_entry(sql, watched_folder_path, key)?;
    }
    sql.execute(
        "DELETE FROM scan_sidecar WHERE watched_folder_path = ? AND folder = ?",
        params![watched_folder_path, folder_text],
    )?;
    let (reason, reason_path) = match &sidecar.files {
        SidecarFiles::Valid(_) => (None, None),
        SidecarFiles::Invalid(reason) => {
            let (reason, path) = invalid_reason_columns(reason);
            (Some(reason), path)
        }
    };
    sql.execute(
        "INSERT INTO scan_sidecar \
             (watched_folder_path, folder, generation, invalid_reason, invalid_reason_path) \
         VALUES (?, ?, ?, ?, ?)",
        params![watched_folder_path, folder_text, generation, reason, reason_path],
    )?;
    if let SidecarFiles::Valid(files) = &sidecar.files {
        for (position, entry) in files.iter().enumerate() {
            let role = match entry.role {
                FileRole::Artwork => "artwork",
                FileRole::Document => "document",
                FileRole::Other => "other",
                FileRole::Audio | FileRole::TrackSheet { .. } => {
                    return Err(DbError::Message(format!(
                        "{} is a sidecar file read as a track or a sheet",
                        entry.file.path.display()
                    )))
                }
            };
            sql.execute(
                "INSERT INTO scan_sidecar_file \
                     (watched_folder_path, folder, position, relative_path, absolute_path, \
                      size, modified_at_ns, role) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    watched_folder_path,
                    folder_text,
                    to_i64(position as u64, "a sidecar file's position")?,
                    entry.file.relative_path,
                    entry.file.path.to_string_lossy(),
                    to_i64(entry.file.size, "a sidecar file's size")?,
                    entry.file.modified_at_ns,
                    role,
                ],
            )?;
        }
    }
    let regrouped = super::super::release_groupings::rebuild_groupings(
        sql,
        &superseded_keys,
        std::slice::from_ref(&sidecar.folder),
        observed_at,
    )?;
    Ok(ScanItemWrite::Stored {
        superseded_keys,
        regrouped,
    })
}

/// Delete every sidecar under `watched_folder_path` whose files an entry
/// covering `coverage` reads itself, and say which folders lost theirs.
pub(super) fn delete_covered_sidecars(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    coverage: &Coverage,
) -> Result<Vec<PathBuf>, DbError> {
    let covered: Vec<PathBuf> = stored_sidecar_folders(sql, watched_folder_path)?
        .into_iter()
        .filter(|folder| {
            coverage.overlaps(&Coverage {
                folder: folder.clone(),
                whole_subtree: false,
            })
        })
        .collect();
    for folder in &covered {
        sql.execute(
            "DELETE FROM scan_sidecar WHERE watched_folder_path = ? AND folder = ?",
            params![watched_folder_path, folder.to_string_lossy()],
        )?;
    }
    Ok(covered)
}

/// Delete every sidecar under `watched_folder_path` — below `under`, when
/// given — that `generation` did not write, and say which folders lost theirs.
pub(super) fn prune_sidecars(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    under: Option<&Path>,
) -> Result<Vec<PathBuf>, DbError> {
    let pruned: Vec<PathBuf> = sql
        .query(
            "SELECT folder FROM scan_sidecar WHERE watched_folder_path = ? AND generation != ? \
             ORDER BY folder",
            params![watched_folder_path, generation],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .map(PathBuf::from)
        .filter(|folder| under.is_none_or(|under| folder.starts_with(under)))
        .collect();
    for folder in &pruned {
        sql.execute(
            "DELETE FROM scan_sidecar WHERE watched_folder_path = ? AND folder = ?",
            params![watched_folder_path, folder.to_string_lossy()],
        )?;
    }
    Ok(pruned)
}

fn stored_sidecar_folders(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
) -> Result<Vec<PathBuf>, DbError> {
    Ok(sql
        .query(
            "SELECT folder FROM scan_sidecar WHERE watched_folder_path = ?",
            [watched_folder_path],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .map(PathBuf::from)
        .collect())
}
