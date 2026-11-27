//! A release's files as a task that reads or moves them takes them: all of
//! them with the release they belong to, or one of them, each read as one
//! statement.

use super::read::{file_of, FileColumns};
use super::*;

// One release's album, the folder it was imported from, its files in the
// order every list of them shows (natural order of their names, case aside,
// id breaking a tie), and the coven file of its cover's browse copy; no row
// for a release the library does not hold.
sqlive_compose::read!(
    OneReleaseWithFiles(release: String) {
        files: String,
        cover_file: Option<coven::FileId>,
    },
    r#"
    SELECT release.album_id, release.source_folder_name,
           (SELECT json_group_array(json(listed.file_json)
                       ORDER BY listed.original_filename COLLATE natural_order,
                                listed.document_file_id)
            FROM @crate::records::file_fragments::FileDocuments(
                     files: (SELECT file.*
                             FROM @crate::records::file_fragments::ReleaseFileColumns AS file
                             WHERE file.release_id = :release)) AS listed) AS files,
           (SELECT cover.browse_file FROM covers AS cover
            WHERE cover.id = release.id) AS cover_file
    FROM releases AS release
    WHERE release.id = :release
"#
);

// The file `file` of the release `release`; no row when the release has no
// such file.
sqlive_compose::read!(
    FileOfRelease(release: String, file: String),
    r#"
    SELECT listed.*
    FROM @crate::records::file_fragments::FileDocuments(
             files: (SELECT file.*
                             FROM @crate::records::file_fragments::ReleaseFileColumns AS file
                             WHERE file.id = :file AND file.release_id = :release)) AS listed
"#
);

/// A release with its files, as the pin, the move to this device and the
/// deletion of a release take them.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseWithFiles {
    pub album_id: String,
    /// The name of the folder the release was imported from.
    pub source_folder_name: Option<String>,
    /// The release's files, in the order every list of them shows.
    pub files: Vec<DbFile>,
    /// The coven file of the cover's browse copy; `None` when the release has
    /// no cover.
    pub cover_file: Option<coven::FileId>,
}

/// The release `release_id` with its files; `None` when the library holds no
/// such release.
pub(super) fn release_with_files_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Option<ReleaseWithFiles>, DbError> {
    let statement = OneReleaseWithFilesParams { release: release_id.to_owned() };
    let Some(row) = bae_store::statement_optional(sql, &statement)? else {
        return Ok(None);
    };
    const DOCUMENT: &str = "a release's files";
    let files = serde_json::from_str::<Vec<FileColumns>>(&row.files)
        .map_err(|error| DbError::unreadable_json(DOCUMENT, &row.files, error))?
        .into_iter()
        .map(file_of_document)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ReleaseWithFiles {
        album_id: row.album_id,
        source_folder_name: row.source_folder_name,
        files,
        cover_file: row.cover_file,
    }))
}

/// The release file `columns` describe, as
/// [`FileDocuments`](super::file_fragments::FileDocuments) wrote them.
fn file_of_document(columns: FileColumns) -> Result<DbFile, DbError> {
    file_of(columns).map_err(|(field, reason)| {
        DbError::unreadable("a release's file", format!("{field}: {reason}"))
    })
}

impl Records {
    /// The release `release_id` with its files; `None` when the library holds
    /// no such release.
    pub async fn find_release_with_files(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseWithFiles>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| release_with_files_on(&sql, &release_id)).await
    }

    /// The release's files, in the order every list of them shows; none for a
    /// release the library does not hold.
    pub async fn get_files_for_release(&self, release_id: &str) -> Result<Vec<DbFile>, DbError> {
        let release = self.find_release_with_files(release_id).await?;
        Ok(release.map(|release| release.files).unwrap_or_default())
    }

    /// The file `file_id` of the release `release_id`; `None` when the release
    /// has no such file.
    pub async fn find_release_file(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<Option<DbFile>, DbError> {
        let statement =
            FileOfReleaseParams { release: release_id.to_owned(), file: file_id.to_owned() };
        self.store
            .read(move |sql| {
                let Some(row) = bae_store::statement_optional(&sql, &statement)? else {
                    return Ok(None);
                };
                let columns = serde_json::from_str(&row.listed.file_json).map_err(|error| {
                    DbError::unreadable_json("a release's file", &row.listed.file_json, error)
                })?;
                file_of_document(columns).map(Some)
            })
            .await
    }
}

#[cfg(test)]
#[path = "release_with_files_tests.rs"]
mod tests;
