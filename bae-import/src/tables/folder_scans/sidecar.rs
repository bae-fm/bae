//! A folder's sidecar files as the scan stores them: the files under a folder
//! that no release read there owns (see
//! [`FolderSidecar`](crate::folder_scanner::FolderSidecar)).
//!
//! A sidecar stands on the same terms as a scan entry. It is written under the
//! scan generation that read it and pruned with the entries that generation
//! did not see. A file on disk is stored in a sidecar or in a scanned release,
//! never both: whichever of the two is written later replaces the other.
//! Whatever changes a sidecar rebuilds the groupings sitting in its folder.

use super::columns::*;
use super::fragments::Sidecars;
use super::*;
use crate::folder_scanner::{
    CandidateFile, FileRole, FileVersion, FolderSidecar, ProposedRole, ScanItem, ScannedFile,
    SidecarFiles, SkippedFile,
};
use crate::CandidateKey;

/// One sidecar file as [`Sidecars`] holds it.
#[derive(serde::Deserialize)]
struct SidecarFileRow {
    relative_path: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    absolute_path: PathBuf,
    size: i64,
    modified_at_ns: i64,
    role: String,
    audio_damage: Option<String>,
    document_kind: Option<String>,
}

/// The sidecar a row of [`Sidecars`] stores.
pub(super) fn sidecar_of(row: Sidecars) -> Result<FolderSidecar, DbError> {
    let folder = path_column::decode(row.sidecar_folder)?;
    let watched_folder_path = path_column::decode(row.sidecar_root)?;
    let rows: Vec<SidecarFileRow> = serde_json::from_str(&row.sidecar_files).map_err(|error| {
        DbError::unreadable_json(
            format!("the sidecar files stored for {}", coven::path_text(&folder)),
            &row.sidecar_files,
            error,
        )
    })?;
    let files = match row.state.as_str() {
        "downloading" => {
            if !rows.is_empty() {
                return Err(DbError::unreadable(
                    format!(
                        "the scan_sidecar_file rows of {}'s downloading sidecar",
                        coven::path_text(&folder)
                    ),
                    rows.len(),
                ));
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
                        role: match (
                            row.role.as_str(),
                            row.audio_damage,
                            row.document_kind.as_deref(),
                        ) {
                            ("artwork", None, None) => FileRole::Artwork,
                            ("damaged_image", None, None) => FileRole::DamagedImage,
                            ("document", None, Some(kind)) => {
                                FileRole::Document { kind: document_kind_of(kind)? }
                            }
                            ("other", None, None) => FileRole::Other,
                            (other, _, _) => {
                                return Err(DbError::unreadable("scan_sidecar_file.role", other))
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
                        role: proposed_role_of(
                            &row.role,
                            row.audio_damage.as_deref(),
                            row.document_kind.as_deref(),
                        )?,
                    })
                })
                .collect::<Result<_, DbError>>()?,
        ),
        other => return Err(DbError::unreadable("scan_sidecar.state", other)),
    };
    Ok(FolderSidecar { watched_folder_path, folder, files })
}

sqlive_compose::read!(
    writer SidecarAt(folder: Vec<u8>),
    r#"
    SELECT sidecar.*
    FROM @crate::tables::folder_scans::fragments::Sidecars AS sidecar
    WHERE sidecar.sidecar_folder = :folder
"#
);

/// The sidecar stored for `folder`.
pub(crate) fn load_sidecar(
    sql: &impl bae_store::WriterRead,
    folder: &Path,
) -> Result<Option<FolderSidecar>, DbError> {
    let request = SidecarAtParams { folder: coven::path_bytes(folder) };
    bae_store::statement_optional(sql, &request)?.map(|row| sidecar_of(row.sidecar)).transpose()
}

sqlive_compose::write!(
    StampSidecar(folder: &'p [u8], generation: i64),
    r#"UPDATE scan_sidecar SET generation = :generation WHERE folder = :folder"#
);

sqlive_compose::write!(
    InsertSidecar(
        folder: &'p [u8],
        watched_folder_path: Vec<u8>,
        generation: i64,
        state: &'static str,
    ),
    r#"
    INSERT INTO scan_sidecar (folder, watched_folder_path, generation, state)
    VALUES (:folder, :watched_folder_path, :generation, :state)
"#
);

sqlive_compose::write!(
    InsertSidecarFile(
        folder: &'p [u8],
        position: i64,
        relative_path: &'p str,
        absolute_path: Vec<u8>,
        size: i64,
        modified_at_ns: i64,
        role: &'static str,
        audio_damage: Option<&'static str>,
        document_kind: Option<&'static str>,
    ),
    r#"
    INSERT INTO scan_sidecar_file
        (folder, position, relative_path, absolute_path, size, modified_at_ns, role,
         audio_damage, document_kind)
    VALUES (:folder, :position, :relative_path, :absolute_path, :size, :modified_at_ns, :role,
        :audio_damage, :document_kind)
"#
);

/// Store `sidecar` under `generation`, replacing the scanned releases that
/// hold any of its files, and rebuild the groupings sitting in its folder or
/// taking in a replaced release. A sidecar stored exactly so only takes the
/// generation.
pub(super) fn write_sidecar(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    sidecar: &FolderSidecar,
) -> Result<ScanItemWrite, DbError> {
    let root = watched_folder_path;
    // The walk finds a sidecar in a folder below the watched folder it walks.
    assert!(
        sidecar.watched_folder_path.as_os_str() == watched_folder_path.as_os_str()
            && sidecar.folder != root
            && sidecar.folder.starts_with(root),
        "the sidecar of {} does not belong below watched folder {}",
        coven::path_text(&sidecar.folder),
        coven::path_text(watched_folder_path)
    );
    let folder_text = coven::path_bytes(&sidecar.folder);
    if load_sidecar(sql, &sidecar.folder)?.as_ref() == Some(sidecar) {
        sql.run(&StampSidecarParams { folder: &folder_text, generation })?;
        return Ok(ScanItemWrite::Unchanged);
    }
    // Every file the sidecar holds, with the role, damage and document kind
    // columns it is stored under. A sidecar holding a damaged track, or two files showing
    // one name, holds every one of its files as the scan read them, and they
    // replace a release holding them as the valid ones do.
    // A folder holding a track or a sheet is a release, never a sidecar.
    let files: Vec<StoredSidecarFile<'_>> = match &sidecar.files {
        SidecarFiles::Valid(files) => files
            .iter()
            .map(|entry| {
                assert!(
                    !matches!(entry.role, FileRole::Audio | FileRole::TrackSheet { .. }),
                    "{} is a sidecar file read as a track or a sheet",
                    coven::path_text(&entry.file.version.path)
                );
                let columns = role_columns(&entry.role);
                StoredSidecarFile {
                    relative_path: &entry.file.relative_path,
                    path: &entry.file.version.path,
                    size: entry.file.version.size,
                    modified_at_ns: entry.file.version.modified_at_ns,
                    role: columns.role,
                    damage: None,
                    document_kind: columns.document_kind,
                }
            })
            .collect(),
        SidecarFiles::Invalid(files) => files
            .iter()
            .map(|file| {
                let columns = proposed_role_columns(file.role);
                assert!(
                    !matches!(file.role, ProposedRole::Audio | ProposedRole::Cue),
                    "{} is a sidecar file read as a track or a sheet",
                    coven::path_text(&file.file.path)
                );
                StoredSidecarFile {
                    relative_path: &file.relative_path,
                    path: &file.file.path,
                    size: file.file.size,
                    modified_at_ns: file.file.modified_at_ns,
                    role: columns.role,
                    damage: columns.audio_damage,
                    document_kind: columns.document_kind,
                }
            })
            .collect(),
        SidecarFiles::Downloading => Vec::new(),
    };
    let paths = path_column::hex_list(files.iter().map(|entry| entry.path));
    let superseded_keys: Vec<CandidateKey> = sql.query(
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
    let touched: Vec<PathBuf> = sql.query(
        "SELECT ? UNION SELECT folder FROM scan_sidecar_file \
         WHERE absolute_path IN (SELECT unhex(value) FROM json_each(?)) \
         ORDER BY 1",
        params![folder_text, paths],
        |row| path_column::get(row, 0),
    )?;
    delete_sidecars_at(sql, &touched)?;
    let state = match &sidecar.files {
        SidecarFiles::Valid(_) => "valid",
        SidecarFiles::Downloading => "downloading",
        SidecarFiles::Invalid(_) => "invalid",
    };
    sql.run(&InsertSidecarParams {
        folder: &folder_text,
        watched_folder_path: coven::path_bytes(watched_folder_path),
        generation,
        state,
    })?;
    for (position, entry) in files.iter().enumerate() {
        sql.run(&InsertSidecarFileParams {
            folder: &folder_text,
            position: to_i64(position as u64, "a sidecar file's position")?,
            relative_path: entry.relative_path,
            absolute_path: coven::path_bytes(entry.path),
            size: to_i64(entry.size, "a sidecar file's size")?,
            modified_at_ns: entry.modified_at_ns,
            role: entry.role,
            audio_damage: entry.damage,
            document_kind: entry.document_kind,
        })?;
    }
    let regrouped =
        crate::tables::release_groupings::rebuild_groupings(sql, &superseded_keys, &touched)?;
    Ok(ScanItemWrite::Stored { superseded_keys, regrouped, found: false })
}

/// One sidecar file as its row stores it.
struct StoredSidecarFile<'a> {
    relative_path: &'a str,
    path: &'a Path,
    size: u64,
    modified_at_ns: i64,
    role: &'static str,
    damage: Option<&'static str>,
    document_kind: Option<&'static str>,
}

// The sidecars of the folders `folders` names, a JSON array of their paths'
// bytes in hex.
sqlive_compose::write!(
    DeleteSidecars(folders: String),
    r#"
    DELETE FROM scan_sidecar
    WHERE folder IN (SELECT unhex(wanted.value) FROM json_each(:folders) AS wanted)
"#
);

/// Delete the sidecars stored for `folders`.
fn delete_sidecars_at(sql: &CovenWrite<'_>, folders: &[PathBuf]) -> Result<(), DbError> {
    sql.run(&DeleteSidecarsParams {
        folders: path_column::hex_list(folders.iter().map(PathBuf::as_path)),
    })?;
    Ok(())
}

/// Delete every sidecar holding one of `item`'s files, now that a scanned
/// release holds them, and say which folders lost theirs.
pub(super) fn delete_sidecars_holding(
    sql: &CovenWrite<'_>,
    item: &ScanItem,
) -> Result<Vec<PathBuf>, DbError> {
    let ScanItem::Valid(candidate) = item else {
        return Ok(Vec::new());
    };
    let held: Vec<PathBuf> = sql.query(
        "SELECT DISTINCT folder FROM scan_sidecar_file \
         WHERE absolute_path IN (SELECT unhex(value) FROM json_each(?)) \
         ORDER BY folder",
        [path_column::hex_list(
            candidate.files.files.iter().map(|entry| entry.file.version.path.as_path()),
        )],
        |row| path_column::get(row, 0),
    )?;
    delete_sidecars_at(sql, &held)?;
    Ok(held)
}

/// Delete every sidecar under `watched_folder_path` — at or below `under`,
/// when given — that `generation` did not write, and say which folders lost
/// theirs.
pub(super) fn prune_sidecars(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    generation: i64,
    under: Option<&Path>,
) -> Result<Vec<PathBuf>, DbError> {
    let pruned: Vec<PathBuf> = match under {
        None => sql.query(
            "SELECT folder FROM scan_sidecar WHERE watched_folder_path = ? AND generation != ? \
             ORDER BY folder",
            params![coven::path_bytes(watched_folder_path), generation],
            |row| path_column::get(row, 0),
        )?,
        Some(under) => {
            let (below_first, below_past) = crate::tables::watched_path::paths_below(under);
            let under = coven::path_bytes(under);
            sql.query(
                "SELECT folder FROM scan_sidecar \
                 WHERE (folder = :folder OR (folder >= :below_first AND folder < :below_past)) \
                   AND watched_folder_path = :root AND generation != :generation \
                 ORDER BY folder",
                rusqlite::named_params! {
                    ":folder": under,
                    ":below_first": below_first,
                    ":below_past": below_past,
                    ":root": coven::path_bytes(watched_folder_path),
                    ":generation": generation,
                },
                |row| path_column::get(row, 0),
            )?
        }
    };
    delete_sidecars_at(sql, &pruned)?;
    Ok(pruned)
}
