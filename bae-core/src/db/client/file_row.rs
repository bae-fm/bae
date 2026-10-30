//! The row a file of coven's is about, as the library holds it now: what
//! the sync status names a pending record's file by.

use super::*;

/// What the row a file is the file of holds, read when the file is listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileRow {
    /// A release file whose row is still in the library, of the release
    /// `release_id`.
    ReleaseFile {
        release_id: String,
        file_name: String,
        album_title: String,
    },
    /// A release's cover whose release is still in the library.
    Cover { album_title: String },
    /// A release file or cover no row of the library names any more: a lost
    /// value naming it keeps its coven file.
    DeletedReleaseFile,
    /// An artist's image.
    ArtistImage,
    /// A person's profile picture.
    Avatar,
}

/// The columns naming the row a coven view's `file.file` is the file of:
/// the file's namespace, then a release file's release, name and album
/// title, then a cover's album title. [`FILE_ROW_JOINS`] reads them.
pub(super) const FILE_ROW_COLUMNS: &str = "named.namespace, named_file.release_id, \
     named_file.original_filename, named_file_album.title, cover_album.title";

/// The joins [`FILE_ROW_COLUMNS`] reads, each by an index on the file the
/// row references. A cover is keyed by its release.
pub(super) const FILE_ROW_JOINS: &str = "\
     LEFT JOIN coven_files AS named ON named.id = file.file \
     LEFT JOIN release_files AS named_file ON named_file.file = file.file \
     LEFT JOIN releases AS named_file_release ON named_file_release.id = named_file.release_id \
     LEFT JOIN albums AS named_file_album ON named_file_album.id = named_file_release.album_id \
     LEFT JOIN covers AS named_cover ON named_cover.file = file.file \
     LEFT JOIN releases AS cover_release ON cover_release.id = named_cover.id \
     LEFT JOIN albums AS cover_album ON cover_album.id = cover_release.album_id";

/// [`FILE_ROW_COLUMNS`] as read, from column `first` on.
pub(super) struct FileRowColumns {
    namespace: Option<String>,
    release_id: Option<String>,
    file_name: Option<String>,
    file_album: Option<String>,
    cover_album: Option<String>,
}

impl FileRowColumns {
    pub(super) fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            namespace: row.get(first)?,
            release_id: row.get(first + 1)?,
            file_name: row.get(first + 2)?,
            file_album: row.get(first + 3)?,
            cover_album: row.get(first + 4)?,
        })
    }

    /// The row they name; `None` when the view names no file this device
    /// has a record of. A namespace bae declares no files in is a broken
    /// record.
    pub(super) fn file_row(self) -> Result<Option<FileRow>, DbError> {
        use crate::sync::{
            ARTIST_IMAGES_NAMESPACE, AVATARS_NAMESPACE, COVERS_NAMESPACE, RELEASE_FILES_NAMESPACE,
        };
        Ok(Some(
            match (
                self.namespace.as_deref(),
                self.release_id,
                self.file_name,
                self.file_album,
                self.cover_album,
            ) {
                (None, ..) => return Ok(None),
                (
                    Some(RELEASE_FILES_NAMESPACE),
                    Some(release_id),
                    Some(file_name),
                    Some(album_title),
                    _,
                ) => FileRow::ReleaseFile {
                    release_id,
                    file_name,
                    album_title,
                },
                (Some(RELEASE_FILES_NAMESPACE), None, None, None, _) => FileRow::DeletedReleaseFile,
                (Some(COVERS_NAMESPACE), _, _, _, Some(album_title)) => {
                    FileRow::Cover { album_title }
                }
                (Some(COVERS_NAMESPACE), _, _, _, None) => FileRow::DeletedReleaseFile,
                (Some(ARTIST_IMAGES_NAMESPACE), ..) => FileRow::ArtistImage,
                (Some(AVATARS_NAMESPACE), ..) => FileRow::Avatar,
                (Some(namespace), _, file_name, file_album, _) => {
                    return Err(DbError::Message(format!(
                        "a file of namespace {namespace} names no row bae holds a file on: \
                         {file_name:?} in {file_album:?}"
                    )))
                }
            },
        ))
    }
}
