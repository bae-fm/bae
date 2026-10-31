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

/// A sidecar as columns of a read over `scan_sidecar` aliased `sidecar`, its
/// files riding on it as a JSON array in their order.
const SIDECAR_COLUMNS: &str = "sidecar.folder, sidecar.watched_folder_path, sidecar.state, \
     (SELECT json_group_array(json_object( \
          'relative_path', file.relative_path, 'absolute_path', hex(file.absolute_path), \
          'size', file.size, 'modified_at_ns', file.modified_at_ns, 'role', file.role, \
          'audio_damage', file.audio_damage) ORDER BY file.position) \
      FROM scan_sidecar_file AS file WHERE file.folder = sidecar.folder) AS files";

/// One sidecar's [`SIDECAR_COLUMNS`] as SQLite hands them over.
struct StoredSidecar {
    folder: String,
    watched_folder_path: String,
    state: String,
    files: String,
}

fn read_stored_sidecar(row: &Row<'_>) -> rusqlite::Result<StoredSidecar> {
    Ok(StoredSidecar {
        folder: row.get(0)?,
        watched_folder_path: row.get(1)?,
        state: row.get(2)?,
        files: row.get(3)?,
    })
}

/// One sidecar file as [`SIDECAR_COLUMNS`] holds it.
#[derive(serde::Deserialize)]
struct SidecarFileRow {
    relative_path: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    absolute_path: PathBuf,
    size: i64,
    modified_at_ns: i64,
    role: String,
    audio_damage: Option<String>,
}

impl StoredSidecar {
    fn sidecar(self) -> Result<FolderSidecar, DbError> {
        let folder = self.folder;
        let rows: Vec<SidecarFileRow> = serde_json::from_str(&self.files).map_err(|error| {
            DbError::Message(format!(
                "the sidecar files stored for {folder} do not read: {error}"
            ))
        })?;
        let files = match self.state.as_str() {
            "downloading" => {
                if !rows.is_empty() {
                    return Err(DbError::Message(format!(
                        "the sidecar of {folder} is downloading and stores files"
                    )));
                }
                SidecarFiles::Downloading
            }
            "valid" => SidecarFiles::Valid(
                rows.into_iter()
                    .map(|row| {
                        Ok(CandidateFile {
                            file: ScannedFile::new(
                                row.absolute_path,
                                row.relative_path,
                                to_u64(row.size, "a sidecar file's size")?,
                                row.modified_at_ns,
                            ),
                            role: match (row.role.as_str(), row.audio_damage) {
                                ("artwork", None) => FileRole::Artwork,
                                ("damaged_image", None) => FileRole::DamagedImage,
                                ("document", None) => FileRole::Document,
                                ("other", None) => FileRole::Other,
                                (other, _) => {
                                    return Err(unreadable("scan_sidecar_file.role", other))
                                }
                            },
                        })
                    })
                    .collect::<Result<_, DbError>>()?,
            ),
            "invalid" => SidecarFiles::Invalid(
                rows.into_iter()
                    .map(|row| {
                        Ok(SkippedFile {
                            relative_path: row.relative_path,
                            file: FileVersion {
                                path: row.absolute_path,
                                size: to_u64(row.size, "a sidecar file's size")?,
                                modified_at_ns: row.modified_at_ns,
                            },
                            role: proposed_role_of(&row.role, row.audio_damage.as_deref())?,
                        })
                    })
                    .collect::<Result<_, DbError>>()?,
            ),
            other => return Err(unreadable("scan_sidecar.state", other)),
        };
        Ok(FolderSidecar {
            watched_folder_path: self.watched_folder_path,
            folder: PathBuf::from(folder),
            files,
        })
    }
}

/// The sidecar stored for `folder`.
pub(crate) fn load_sidecar(
    sql: &(impl QueryOne + QueryRows),
    folder: &Path,
) -> Result<Option<FolderSidecar>, DbError> {
    sql.query_row(
        &format!("SELECT {SIDECAR_COLUMNS} FROM scan_sidecar AS sidecar WHERE sidecar.folder = ?"),
        [folder.to_string_lossy()],
        read_stored_sidecar,
    )
    .optional()?
    .map(StoredSidecar::sidecar)
    .transpose()
}

/// Every sidecar stored under `watched_folder_path`, in folder order.
pub(super) fn load_sidecars(
    sql: &(impl QueryOne + QueryRows),
    watched_folder_path: &str,
) -> Result<Vec<FolderSidecar>, DbError> {
    sql.query(
        &format!(
            "SELECT {SIDECAR_COLUMNS} FROM scan_sidecar AS sidecar \
             WHERE sidecar.watched_folder_path = ? ORDER BY sidecar.folder"
        ),
        [watched_folder_path],
        read_stored_sidecar,
    )?
    .into_iter()
    .map(StoredSidecar::sidecar)
    .collect()
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
    // stored under. A sidecar holding a damaged track, or two files showing
    // one name, holds every one of its files as the scan read them, and they
    // replace a release holding them as the valid ones do.
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
    let paths = path_column::hex_list(files.iter().map(|entry| entry.path));
    let superseded_keys: Vec<String> = sql.query(
        "SELECT DISTINCT file.candidate_path FROM scan_candidate_file AS file \
         JOIN scan_candidate AS candidate \
           ON candidate.watched_folder_path = file.watched_folder_path \
          AND candidate.path = file.candidate_path \
         WHERE file.absolute_path IN (SELECT unhex(value) FROM json_each(?)) \
           AND candidate.source_kind = 'folder' \
         ORDER BY file.candidate_path",
        [&paths],
        |row| row.get(0),
    )?;
    for key in &superseded_keys {
        delete_entry(sql, watched_folder_path, key)?;
    }
    // A sidecar an earlier reading stored for another folder that held one
    // of these files — the folder above, before this one held audio below
    // it — is replaced the same way.
    let touched: Vec<String> = sql.query(
        "SELECT ? UNION SELECT folder FROM scan_sidecar_file \
         WHERE absolute_path IN (SELECT unhex(value) FROM json_each(?)) \
         ORDER BY 1",
        params![folder_text, paths],
        |row| row.get(0),
    )?;
    delete_sidecars_at(sql, &touched)?;
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
                coven::path_bytes(entry.path),
                to_i64(entry.size, "a sidecar file's size")?,
                entry.modified_at_ns,
                entry.role,
                entry.damage,
            ],
        )?;
    }
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

/// Delete the sidecars stored for `folders`.
fn delete_sidecars_at(sql: &SqlContext<'_, '_>, folders: &[String]) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM scan_sidecar WHERE folder IN (SELECT value FROM json_each(?))",
        [serde_json::to_string(folders).expect("folder text serializes as JSON")],
    )?;
    Ok(())
}

/// Delete every sidecar holding one of `item`'s files, now that a scanned
/// release holds them, and say which folders lost theirs.
pub(super) fn delete_sidecars_holding(
    sql: &SqlContext<'_, '_>,
    item: &ScanItem,
) -> Result<Vec<String>, DbError> {
    let ScanItem::Valid(candidate) = item else {
        return Ok(Vec::new());
    };
    let held: Vec<String> = sql.query(
        "SELECT DISTINCT folder FROM scan_sidecar_file \
         WHERE absolute_path IN (SELECT unhex(value) FROM json_each(?)) \
         ORDER BY folder",
        [path_column::hex_list(
            candidate
                .files
                .files
                .iter()
                .map(|entry| entry.file.path.as_path()),
        )],
        |row| row.get(0),
    )?;
    delete_sidecars_at(sql, &held)?;
    Ok(held)
}

/// Delete every sidecar under `watched_folder_path` — at or below `under`,
/// when given — that `generation` did not write, and say which folders lost
/// theirs.
pub(super) fn prune_sidecars(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    generation: i64,
    under: Option<&Path>,
) -> Result<Vec<String>, DbError> {
    let pruned: Vec<String> = match under {
        None => sql.query(
            "SELECT folder FROM scan_sidecar WHERE watched_folder_path = ? AND generation != ? \
             ORDER BY folder",
            params![watched_folder_path, generation],
            |row| row.get(0),
        )?,
        Some(under) => {
            let under = under.to_string_lossy();
            let (below_first, below_past) = crate::import::folder_path::paths_below(&under);
            sql.query(
                "SELECT folder FROM scan_sidecar \
                 WHERE (folder = :folder OR (folder >= :below_first AND folder < :below_past)) \
                   AND watched_folder_path = :root AND generation != :generation \
                 ORDER BY folder",
                rusqlite::named_params! {
                    ":folder": under,
                    ":below_first": below_first,
                    ":below_past": below_past,
                    ":root": watched_folder_path,
                    ":generation": generation,
                },
                |row| row.get(0),
            )?
        }
    };
    delete_sidecars_at(sql, &pruned)?;
    Ok(pruned)
}
