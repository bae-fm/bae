//! A folder's sidecar files as the scan stores them: the files under a folder
//! that no release read there owns (see
//! [`FolderSidecar`](crate::import::folder_scanner::FolderSidecar)).
//!
//! A sidecar stands on the same terms as a scan entry. It is written under the
//! scan generation that read it and pruned with the entries that generation
//! did not see. A file on disk is stored in a sidecar or in a scanned release,
//! never both: whichever of the two is written later replaces the other.
//! Whatever changes a sidecar rebuilds the groupings sitting in its folder.

use super::columns::*;
use super::*;
use crate::import::folder_scanner::{
    CandidateFile, FileRole, FileVersion, FolderSidecar, ProposedRole, ScanItem, ScannedFile,
    SidecarFiles, SkippedFile,
};
use std::collections::BTreeSet;

/// The sidecar stored for `folder`.
pub(crate) fn load_sidecar(
    sql: &(impl QueryOne + QueryRows),
    folder: &Path,
) -> Result<Option<FolderSidecar>, DbError> {
    let folder_text = folder.to_string_lossy();
    let row: Option<(String, String)> = sql
        .query_row(
            "SELECT watched_folder_path, state FROM scan_sidecar WHERE folder = ?",
            [&folder_text],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((watched_folder_path, state)) = row else {
        return Ok(None);
    };
    let rows = sql.query(
        "SELECT relative_path, absolute_path, size, modified_at_ns, role, audio_damage \
         FROM scan_sidecar_file WHERE folder = ? ORDER BY position",
        [&folder_text],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        },
    )?;
    let files = match state.as_str() {
        "downloading" => {
            if !rows.is_empty() {
                return Err(DbError::Message(format!(
                    "the sidecar of {folder_text} is downloading and stores files"
                )));
            }
            SidecarFiles::Downloading
        }
        "valid" => SidecarFiles::Valid(
            rows.into_iter()
                .map(
                    |(relative_path, absolute_path, size, modified_at_ns, role, damage)| {
                        Ok(CandidateFile {
                            file: ScannedFile::new(
                                PathBuf::from(absolute_path),
                                relative_path,
                                to_u64(size, "a sidecar file's size")?,
                                modified_at_ns,
                            ),
                            role: match (role.as_str(), damage) {
                                ("artwork", None) => FileRole::Artwork,
                                ("damaged_image", None) => FileRole::DamagedImage,
                                ("document", None) => FileRole::Document,
                                ("other", None) => FileRole::Other,
                                (other, _) => {
                                    return Err(unreadable("scan_sidecar_file.role", other))
                                }
                            },
                        })
                    },
                )
                .collect::<Result<_, DbError>>()?,
        ),
        "invalid" => SidecarFiles::Invalid(
            rows.into_iter()
                .map(
                    |(relative_path, absolute_path, size, modified_at_ns, role, damage)| {
                        Ok(SkippedFile {
                            relative_path,
                            file: FileVersion {
                                path: PathBuf::from(absolute_path),
                                size: to_u64(size, "a sidecar file's size")?,
                                modified_at_ns,
                            },
                            role: proposed_role_of(&role, damage.as_deref())?,
                        })
                    },
                )
                .collect::<Result<_, DbError>>()?,
        ),
        other => return Err(unreadable("scan_sidecar.state", other)),
    };
    Ok(Some(FolderSidecar {
        watched_folder_path,
        folder: folder.to_path_buf(),
        files,
    }))
}

/// Store `sidecar` under `generation`, replacing the scanned releases that
/// hold any of its files, and rebuild the groupings sitting in its folder or
/// taking in a replaced release. A sidecar stored exactly so only takes the
/// generation.
pub(super) fn write_sidecar(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    sidecar: &FolderSidecar,
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
            "UPDATE scan_sidecar SET generation = ? WHERE folder = ?",
            params![generation, folder_text],
        )?;
        return Ok(ScanItemWrite::Unchanged);
    }
    // Every file the sidecar holds, with the role and damage columns it is
    // stored under. A sidecar holding a damaged track holds every one of its
    // files as the scan read them, and they replace a release holding them
    // as the valid ones do.
    let files: Vec<StoredSidecarFile<'_>> = match &sidecar.files {
        SidecarFiles::Valid(files) => files
            .iter()
            .map(|entry| {
                Ok(StoredSidecarFile {
                    relative_path: &entry.file.relative_path,
                    path: &entry.file.path,
                    size: entry.file.size,
                    modified_at_ns: entry.file.modified_at_ns,
                    role: match entry.role {
                        FileRole::Artwork => "artwork",
                        FileRole::DamagedImage => "damaged_image",
                        FileRole::Document => "document",
                        FileRole::Other => "other",
                        FileRole::Audio | FileRole::TrackSheet { .. } => {
                            return Err(DbError::Message(format!(
                                "{} is a sidecar file read as a track or a sheet",
                                entry.file.path.display()
                            )))
                        }
                    },
                    damage: None,
                })
            })
            .collect::<Result<_, DbError>>()?,
        SidecarFiles::Invalid(files) => files
            .iter()
            .map(|file| {
                let (role, damage) = proposed_role_columns(file.role);
                if matches!(file.role, ProposedRole::Audio | ProposedRole::Cue) {
                    return Err(DbError::Message(format!(
                        "{} is a sidecar file read as a track or a sheet",
                        file.file.path.display()
                    )));
                }
                Ok(StoredSidecarFile {
                    relative_path: &file.relative_path,
                    path: &file.file.path,
                    size: file.file.size,
                    modified_at_ns: file.file.modified_at_ns,
                    role,
                    damage,
                })
            })
            .collect::<Result<_, DbError>>()?,
        SidecarFiles::Downloading => Vec::new(),
    };
    let mut superseded_keys = BTreeSet::new();
    for entry in &files {
        superseded_keys.extend(sql.query(
            "SELECT file.candidate_path FROM scan_candidate_file AS file \
             JOIN scan_candidate AS candidate \
               ON candidate.watched_folder_path = file.watched_folder_path \
              AND candidate.path = file.candidate_path \
             WHERE file.absolute_path = ? AND candidate.source_kind = 'folder'",
            [entry.path.to_string_lossy()],
            |row| row.get::<_, String>(0),
        )?);
    }
    let superseded_keys: Vec<String> = superseded_keys.into_iter().collect();
    for key in &superseded_keys {
        delete_entry(sql, watched_folder_path, key)?;
    }
    // A sidecar an earlier reading stored for another folder that held one
    // of these files — the folder above, before this one held audio below
    // it — is replaced the same way.
    let mut touched = BTreeSet::from([sidecar.folder.clone()]);
    for entry in &files {
        touched.extend(
            sql.query(
                "SELECT folder FROM scan_sidecar_file WHERE absolute_path = ?",
                [entry.path.to_string_lossy()],
                |row| row.get::<_, String>(0),
            )?
            .into_iter()
            .map(PathBuf::from),
        );
    }
    for folder in &touched {
        sql.execute(
            "DELETE FROM scan_sidecar WHERE folder = ?",
            [folder.to_string_lossy()],
        )?;
    }
    let state = match &sidecar.files {
        SidecarFiles::Valid(_) => "valid",
        SidecarFiles::Downloading => "downloading",
        SidecarFiles::Invalid(_) => "invalid",
    };
    sql.execute(
        "INSERT INTO scan_sidecar (folder, watched_folder_path, generation, state) \
         VALUES (?, ?, ?, ?)",
        params![folder_text, watched_folder_path, generation, state],
    )?;
    for (position, entry) in files.iter().enumerate() {
        sql.execute(
            "INSERT INTO scan_sidecar_file \
                 (folder, position, relative_path, absolute_path, size, modified_at_ns, role, \
                  audio_damage) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                folder_text,
                to_i64(position as u64, "a sidecar file's position")?,
                entry.relative_path,
                entry.path.to_string_lossy(),
                to_i64(entry.size, "a sidecar file's size")?,
                entry.modified_at_ns,
                entry.role,
                entry.damage,
            ],
        )?;
    }
    let touched: Vec<PathBuf> = touched.into_iter().collect();
    let regrouped =
        super::super::release_groupings::rebuild_groupings(sql, &superseded_keys, &touched)?;
    Ok(ScanItemWrite::Stored {
        superseded_keys,
        regrouped,
        found: false,
    })
}

/// One sidecar file as its row stores it.
struct StoredSidecarFile<'a> {
    relative_path: &'a str,
    path: &'a Path,
    size: u64,
    modified_at_ns: i64,
    role: &'static str,
    damage: Option<&'static str>,
}

/// Delete every sidecar holding one of `item`'s files, now that a scanned
/// release holds them, and say which folders lost theirs.
pub(super) fn delete_sidecars_holding(
    sql: &SqlContext<'_, '_>,
    item: &ScanItem,
) -> Result<Vec<PathBuf>, DbError> {
    let ScanItem::Valid(candidate) = item else {
        return Ok(Vec::new());
    };
    let mut held = BTreeSet::new();
    for entry in &candidate.files.files {
        held.extend(sql.query(
            "SELECT folder FROM scan_sidecar_file WHERE absolute_path = ?",
            [entry.file.path.to_string_lossy()],
            |row| row.get::<_, String>(0),
        )?);
    }
    for folder in &held {
        sql.execute("DELETE FROM scan_sidecar WHERE folder = ?", [folder])?;
    }
    Ok(held.into_iter().map(PathBuf::from).collect())
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
            "DELETE FROM scan_sidecar WHERE folder = ?",
            [folder.to_string_lossy()],
        )?;
    }
    Ok(pruned)
}
