//! The row a file of coven's is about, as the library holds it now: what
//! the sync status names a pending record's or an abandoned attachment's
//! file by.

use super::*;

/// What the row a file is the file of holds, read when the file is listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileRow {
    /// A release file whose row is still in the library.
    ReleaseFile {
        file_name: String,
        album_title: String,
    },
    /// A release's cover whose release is still in the library.
    Cover { album_title: String },
    /// A release file or cover whose row or release is gone.
    DeletedReleaseFile,
    /// An artist's image.
    ArtistImage,
    /// A person's profile picture.
    Avatar,
}

/// The columns naming the row `file.table_name` and `file.key` say a file
/// is about, in coven's views: the table, then a release file's name and
/// album title, then a cover's album title. [`FILE_ROW_JOINS`] reads them.
pub(super) const FILE_ROW_COLUMNS: &str =
    "file.table_name, named_file.original_filename, named_file_album.title, cover_album.title";

/// The joins [`FILE_ROW_COLUMNS`] reads, each by an index on `coven_key`. A
/// cover is keyed by its release.
pub(super) const FILE_ROW_JOINS: &str = "\
     LEFT JOIN release_files AS named_file \
         ON file.table_name = 'release_files' AND coven_key(named_file.id) = file.key \
     LEFT JOIN releases AS named_file_release ON named_file_release.id = named_file.release_id \
     LEFT JOIN albums AS named_file_album ON named_file_album.id = named_file_release.album_id \
     LEFT JOIN releases AS cover_release \
         ON file.table_name = 'covers' AND coven_key(cover_release.id) = file.key \
     LEFT JOIN albums AS cover_album ON cover_album.id = cover_release.album_id";

/// [`FILE_ROW_COLUMNS`] as read, from column `first` on.
pub(super) struct FileRowColumns {
    table: Option<String>,
    file_name: Option<String>,
    file_album: Option<String>,
    cover_album: Option<String>,
}

impl FileRowColumns {
    pub(super) fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            table: row.get(first)?,
            file_name: row.get(first + 1)?,
            file_album: row.get(first + 2)?,
            cover_album: row.get(first + 3)?,
        })
    }

    /// The row they name; `None` when the view names no row. A table bae
    /// declares no file on is a broken record.
    pub(super) fn file_row(self) -> Result<Option<FileRow>, DbError> {
        Ok(Some(
            match (
                self.table.as_deref(),
                self.file_name,
                self.file_album,
                self.cover_album,
            ) {
                (None, ..) => return Ok(None),
                (Some("release_files"), Some(file_name), Some(album_title), _) => {
                    FileRow::ReleaseFile {
                        file_name,
                        album_title,
                    }
                }
                (Some("release_files"), None, None, _) => FileRow::DeletedReleaseFile,
                (Some("covers"), _, _, Some(album_title)) => FileRow::Cover { album_title },
                (Some("covers"), _, _, None) => FileRow::DeletedReleaseFile,
                (Some("artist_images"), ..) => FileRow::ArtistImage,
                (Some("people"), ..) => FileRow::Avatar,
                (Some(table), file_name, file_album, _) => {
                    return Err(DbError::Message(format!(
                        "a file of {table} names no row bae holds a file on: \
                         {file_name:?} in {file_album:?}"
                    )))
                }
            },
        ))
    }
}
