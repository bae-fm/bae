//! A folder that failed validation, as its row under Skipped and the pane
//! looking into it read it: what its scan row stores. The scan judged the
//! folder's files when it wrote the row, so no read judges them again.

use super::columns::proposed_role_of;
use super::*;
use crate::folder_scanner::{FileVersion, InvalidReason, ProposedRole, SkippedTags};
use crate::triage::InvalidRow;
use crate::CandidateKey;
use crate::{InvalidCandidateDetail, MappingFile, MappingImage, MappingRole};

/// What a read of a window's rows adds to a row of `scan_candidate`, joined
/// as `candidate`, to show it as a folder that failed validation: the columns
/// to select, and the joins they read through. The scan stored why the folder
/// failed and the image it leads with in the row; the cover join reads the
/// version of that image the scan read, the first file by that name among
/// the folder's images.
pub(crate) fn invalid_columns_sql(candidate: &str) -> (String, String) {
    (
        format!(
            "{candidate}.folder AS invalid_folder, \
             {candidate}.grouping_key IS NOT NULL AS invalid_separable, \
             {candidate}.invalid_reason AS invalid_reason, \
             {candidate}.invalid_shared_name AS invalid_shared_name, \
             {candidate}.invalid_damaged_audio AS invalid_damaged_audio, \
             invalid_tags.album_title AS invalid_album_title, \
             invalid_tags.credits AS invalid_credits, \
             invalid_cover.absolute_path AS invalid_cover_path, \
             invalid_cover.size AS invalid_cover_size, \
             invalid_cover.modified_at_ns AS invalid_cover_modified"
        ),
        format!(
            "LEFT JOIN scan_skipped_tags AS invalid_tags \
                 ON invalid_tags.watched_folder_path = {candidate}.watched_folder_path \
                AND invalid_tags.candidate_path = {candidate}.path \
             LEFT JOIN scan_skipped_file AS invalid_cover \
                 ON {candidate}.kind = 'invalid' \
                AND invalid_cover.watched_folder_path = {candidate}.watched_folder_path \
                AND invalid_cover.candidate_path = {candidate}.path \
                AND invalid_cover.position = ( \
                    SELECT MIN(image.position) FROM scan_skipped_file AS image \
                    WHERE image.watched_folder_path = {candidate}.watched_folder_path \
                      AND image.candidate_path = {candidate}.path \
                      AND image.role = 'artwork' \
                      AND image.relative_path = {candidate}.blank_cover_file)"
        ),
    )
}

/// A folder that failed validation, as [`invalid_columns_sql`] selects it.
#[derive(Clone, PartialEq)]
pub(crate) struct InvalidColumns {
    folder: PathBuf,
    separable: bool,
    reason: (String, Option<String>, Option<i64>),
    tags: Option<SkippedTags>,
    cover: Option<(PathBuf, i64, i64)>,
}

impl InvalidColumns {
    /// The columns of a row of another kind hold no reason, which reads as
    /// `None`.
    pub(crate) fn read(row: &rusqlite::Row<'_>) -> Result<Option<Self>, rusqlite::Error> {
        let Some(reason) = row.get::<_, Option<String>>("invalid_reason")? else {
            return Ok(None);
        };
        let cover = match path_column::get_optional(row, "invalid_cover_path")? {
            Some(path) => {
                Some((path, row.get("invalid_cover_size")?, row.get("invalid_cover_modified")?))
            }
            None => None,
        };
        Ok(Some(Self {
            folder: path_column::get(row, "invalid_folder")?,
            separable: row.get("invalid_separable")?,
            reason: (reason, row.get("invalid_shared_name")?, row.get("invalid_damaged_audio")?),
            tags: row
                .get::<_, Option<String>>("invalid_album_title")?
                .map(|album_title| {
                    Ok::<_, rusqlite::Error>(SkippedTags {
                        album_title,
                        artist: crate::tables::stored_credits::line_column(row, "invalid_credits")?,
                    })
                })
                .transpose()?,
            cover,
        }))
    }

    /// The row of the folder stored at `key` under `watched_folder_path`,
    /// named `folder_name`.
    pub(crate) fn row(
        self,
        key: CandidateKey,
        folder_name: String,
        watched_folder_path: PathBuf,
    ) -> Result<InvalidRow, DbError> {
        let reason = match self.reason {
            (kind, Some(name), None) if kind == "same_name" => InvalidReason::SameName { name },
            (kind, None, Some(count)) if kind == "damaged_audio" => InvalidReason::DamagedAudio {
                count: u32::try_from(count).map_err(|_too_large| {
                    DbError::unreadable("scan_candidate.invalid_damaged_audio", count)
                })?,
            },
            (kind, None, None) if kind == "no_valid_audio" => InvalidReason::NoValidAudio,
            other => return Err(DbError::unreadable("scan_candidate.invalid_reason", other)),
        };
        let cover = match self.cover {
            Some((path, size, modified_at_ns)) => Some(crate::CoverImageSource::Local {
                file: FileVersion {
                    path,
                    size: to_u64(size, "a skipped image's size")?,
                    modified_at_ns,
                },
            }),
            None => None,
        };
        Ok(InvalidRow {
            candidate_key: key,
            folder: self.folder,
            folder_name,
            watched_folder_path,
            separable: self.separable,
            reason,
            tags: self.tags,
            cover,
        })
    }
}

/// The folder that failed validation stored at `key`, as its row shows it;
/// `None` when no folder that failed validation is stored there. Watched
/// roots never overlap and keys are absolute paths, so at most one root
/// holds it.
pub(crate) fn load_invalid_row(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<Option<InvalidRow>, DbError> {
    let (columns, joins) = invalid_columns_sql("c");
    let read = sql
        .query_row(
            &format!(
                "SELECT c.watched_folder_path, c.path, c.name, {columns} \
                 FROM scan_candidate AS c {joins} \
                 WHERE c.path = ? AND c.kind = 'invalid'"
            ),
            [key],
            |row| {
                Ok((
                    path_column::get(row, "watched_folder_path")?,
                    row.get::<_, CandidateKey>("path")?,
                    row.get::<_, String>("name")?,
                    InvalidColumns::read(row)?,
                ))
            },
        )
        .optional()?;
    let Some((root, key, name, columns)) = read else {
        return Ok(None);
    };
    // The schema holds a reason on every row of an invalid folder.
    let columns = columns.ok_or_else(|| {
        DbError::unreadable(format!("the reason folder {key} failed validation"), None::<String>)
    })?;
    columns.row(key, name, root).map(Some)
}

/// The files of the folder that failed validation stored at the key bound
/// as `?1`, joined as `f`, for a statement that adds which roles it keeps.
const INVALID_FILES: &str = "FROM scan_candidate AS c \
     JOIN scan_skipped_file AS f \
       ON f.watched_folder_path = c.watched_folder_path AND f.candidate_path = c.path \
     WHERE c.path = ?1 AND c.kind = 'invalid'";

/// The folder that failed validation stored at `key`, as the pane looking
/// into it reads it: its row, the images that decode, and every other file,
/// each in name order. Each statement reads by the key the request names, and
/// SQL keeps each part's files by their role; `None` when no folder that
/// failed validation is stored there.
pub(crate) fn load_invalid_detail(
    sql: &impl coven::SqlRead,
    key: &CandidateKey,
) -> Result<Option<InvalidCandidateDetail>, DbError> {
    let row = load_invalid_row(sql, key)?;
    let images = sql.query(
        &format!(
            "SELECT f.relative_path, f.absolute_path, f.size, f.modified_at_ns {INVALID_FILES} \
               AND f.role = 'artwork' \
             ORDER BY f.position"
        ),
        [key],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                path_column::get(row, 1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )?;
    let files = sql.query(
        &format!(
            "SELECT f.relative_path, f.absolute_path, f.size, f.role, f.audio_damage, \
                    f.document_kind {INVALID_FILES} \
               AND f.role <> 'artwork' \
             ORDER BY f.position"
        ),
        [key],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                path_column::get(row, 1)?,
                row.get::<_, i64>(2)?,
                (
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ),
            ))
        },
    )?;
    let Some(candidate) = row else {
        return Ok(None);
    };
    let images = images
        .into_iter()
        .map(|(relative_path, path, size, modified_at_ns)| {
            Ok(MappingImage {
                name: file_name(&relative_path).to_string(),
                file_id: relative_path,
                file: FileVersion {
                    path,
                    size: to_u64(size, "a skipped image's size")?,
                    modified_at_ns,
                },
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let files = files
        .into_iter()
        .map(|(relative_path, path, size, (role, damage, document_kind))| {
            let role =
                mapping_role(proposed_role_of(&role, damage.as_deref(), document_kind.as_deref())?);
            Ok(MappingFile {
                file_id: relative_path.clone(),
                name: relative_path,
                size: to_u64(size, "a skipped file's size")?,
                preview_target: (role == MappingRole::Audio)
                    .then(|| bae_track_audio::PreviewTarget::whole_file(path)),
                role,
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(Some(InvalidCandidateDetail { candidate, images, files }))
}

/// What the pane lists a skipped file as. bae never imports a skipped folder,
/// so it never binds a cue sheet in one to audio, and the pane lists the
/// sheet as a document.
fn mapping_role(role: ProposedRole) -> MappingRole {
    match role {
        ProposedRole::Audio => MappingRole::Audio,
        ProposedRole::DamagedAudio { damage } => MappingRole::DamagedAudio { damage },
        ProposedRole::Cue | ProposedRole::Document { .. } => MappingRole::Document,
        ProposedRole::Image => MappingRole::Image,
        ProposedRole::DamagedImage => MappingRole::DamagedImage,
        ProposedRole::Other => MappingRole::Other,
    }
}

/// The last name of a `/`-separated relative path: the file's own name,
/// without its folders.
fn file_name(relative_path: &str) -> &str {
    relative_path.rsplit('/').next().unwrap_or(relative_path)
}
