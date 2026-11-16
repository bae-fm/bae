//! A folder that failed validation, as its row under Skipped and the pane
//! looking into it read it: what its scan row stores. The scan judged the
//! folder's files when it wrote the row, so no read judges them again.

use super::*;
use crate::folder_scanner::{FileVersion, InvalidReason, SkippedTags};
use crate::triage::InvalidRow;
use crate::CandidateKey;

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
             coven_path_text({candidate}.relative_folder) AS invalid_display_path, \
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
    display_path: String,
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
            display_path: row.get("invalid_display_path")?,
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
            display_path: self.display_path,
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
