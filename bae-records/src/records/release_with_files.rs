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
    writer OneReleaseWithFiles(release: String) {
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
    writer FileOfRelease(release: String, file: String),
    r#"
    SELECT listed.*
    FROM @crate::records::file_fragments::FileDocuments(
             files: (SELECT file.*
                             FROM @crate::records::file_fragments::ReleaseFileColumns AS file
                             WHERE file.id = :file AND file.release_id = :release)) AS listed
"#
);

// The release file `file`, of whichever release holds it.
sqlive_compose::read!(
    writer ReleaseFileById(file: String),
    r#"
    SELECT record.*
    FROM @crate::records::file_fragments::ReleaseFileRecord AS record
    WHERE record.id = :file
"#
);

impl super::file_fragments::ReleaseFileRecord {
    /// The release file the row describes.
    pub fn into_file(self) -> Result<DbFile, DbError> {
        let listed = self.listed;
        let missing = |what: &str| {
            DbError::unreadable(format!("the {what} of file {}", listed.id), None::<String>)
        };
        let file_size = self.file_size.ok_or_else(|| missing("size"))?;
        let stored = self.stored.ok_or_else(|| missing("stored state"))?;
        decoded_file(FileColumns {
            id: listed.id,
            release_id: listed.release_id,
            original_filename: listed.original_filename,
            content_type: listed.content_type,
            created_at: listed.created_at,
            file: listed.file,
            role: listed.role,
            document_kind: listed.document_kind,
            source_audio_layout: listed.source_audio_layout,
            source_audio_content_type: listed.source_audio_content_type,
            source_audio_container: listed.source_audio_container,
            source_audio_duration_ms: listed.source_audio_duration_ms,
            source_audio_sample_rate_hz: listed.source_audio_sample_rate_hz,
            source_audio_bits_per_sample: listed.source_audio_bits_per_sample,
            source_audio_bitrate_kbps: listed.source_audio_bitrate_kbps,
            source_audio_channels: listed.source_audio_channels,
            file_size,
            stored: i64::from(stored),
        })
    }
}

/// The release file `file_id`, of whichever release holds it; `None` when the
/// library holds no such file.
pub fn release_file_on(
    sql: &impl bae_store::WriterRead,
    file_id: &str,
) -> Result<Option<DbFile>, DbError> {
    let statement = ReleaseFileByIdParams { file: file_id.to_owned() };
    bae_store::statement_optional(sql, &statement)?.map(|row| row.record.into_file()).transpose()
}

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
    sql: &impl bae_store::WriterRead,
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
        .map(decoded_file)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ReleaseWithFiles {
        album_id: row.album_id,
        source_folder_name: row.source_folder_name,
        files,
        cover_file: row.cover_file,
    }))
}

/// The release file `columns` describe.
fn decoded_file(columns: FileColumns) -> Result<DbFile, DbError> {
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
                decoded_file(columns).map(Some)
            })
            .await
    }
}

#[cfg(test)]
#[path = "release_with_files_tests.rs"]
mod tests;
