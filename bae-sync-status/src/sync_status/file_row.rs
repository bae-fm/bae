//! The row a file of coven's is about, as the library holds it now: what
//! the sync status names a pending record's file by.

use super::*;

/// What the row a file is the file of holds, read when the file is listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRow {
    /// A release file whose row is still in the library, of the release
    /// `release_id`.
    ReleaseFile { release_id: String, file_name: String, album_title: String },
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

/// The columns naming the release file a pending record is about: its
/// release, its name and its album's title. A move's record names the release
/// file of its original, and a file's record the release file of its file
/// when the file's namespace is the release files'. The columns are `NULL`
/// unless all three are present. This is the one place that rule lives: the
/// pending rows' files and the failure's file both read these columns.
pub(super) fn release_file_columns() -> ReleaseFileColumns {
    use bae_records::synced::RELEASE_FILES_NAMESPACE;
    let picked = |original: &str, named: &str| {
        format!(
            "CASE WHEN file.reason_kind = 'original' THEN {original} \
             WHEN named.namespace = '{RELEASE_FILES_NAMESPACE}' THEN {named} END"
        )
    };
    let id = picked("original_file.release_id", "named_file.release_id");
    let name = picked("original_file.original_filename", "named_file.original_filename");
    let album = picked("original_album.title", "named_file_album.title");
    let present = format!("{id} IS NOT NULL AND {name} IS NOT NULL AND {album} IS NOT NULL");
    let only_if_present = |column: String| format!("CASE WHEN {present} THEN {column} END");
    ReleaseFileColumns {
        release_id: only_if_present(id.clone()),
        file_name: only_if_present(name.clone()),
        album_title: only_if_present(album.clone()),
    }
}

/// The SQL of [`release_file_columns`]' three columns.
pub(super) struct ReleaseFileColumns {
    pub(super) release_id: String,
    pub(super) file_name: String,
    pub(super) album_title: String,
}

/// The columns naming the row a coven view's `file.file` is the file of:
/// the file's namespace, then the release file ([`release_file_columns`]),
/// then a cover's album title. [`FILE_ROW_JOINS`] reads them.
pub(super) fn file_row_columns() -> String {
    let ReleaseFileColumns { release_id, file_name, album_title } = release_file_columns();
    format!("named.namespace, {release_id}, {file_name}, {album_title}, cover_album.title")
}

/// The joins [`file_row_columns`] reads, each by an index on the file the
/// row references. A cover is keyed by its release.
pub(super) const FILE_ROW_JOINS: &str = "\
     LEFT JOIN coven_files AS named ON named.id = file.file \
     LEFT JOIN release_files AS named_file ON named_file.file = file.file \
     LEFT JOIN releases AS named_file_release ON named_file_release.id = named_file.release_id \
     LEFT JOIN albums AS named_file_album ON named_file_album.id = named_file_release.album_id \
     LEFT JOIN covers AS named_cover ON named_cover.file = file.file \
     LEFT JOIN releases AS cover_release ON cover_release.id = named_cover.id \
     LEFT JOIN albums AS cover_album ON cover_album.id = cover_release.album_id";

/// [`file_row_columns`] as read, from column `first` on.
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

    /// The release file a move's record names by its original, when the
    /// columns hold one.
    pub(super) fn original_release_file(self) -> Option<FileRow> {
        match (self.release_id, self.file_name, self.file_album) {
            (Some(release_id), Some(file_name), Some(album_title)) => {
                Some(FileRow::ReleaseFile { release_id, file_name, album_title })
            }
            _ => None,
        }
    }

    /// The row they name; `None` when the view names no file this device
    /// has a record of. A namespace bae declares no files in is a broken
    /// record.
    pub(super) fn file_row(self) -> Result<Option<FileRow>, DbError> {
        use bae_people::synced::AVATARS_NAMESPACE;
        use bae_records::synced::{
            ARTIST_IMAGES_NAMESPACE, COVERS_NAMESPACE, RELEASE_FILES_NAMESPACE,
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
                ) => FileRow::ReleaseFile { release_id, file_name, album_title },
                (Some(RELEASE_FILES_NAMESPACE), None, None, None, _) => FileRow::DeletedReleaseFile,
                (Some(COVERS_NAMESPACE), _, _, _, Some(album_title)) => {
                    FileRow::Cover { album_title }
                }
                (Some(COVERS_NAMESPACE), _, _, _, None) => FileRow::DeletedReleaseFile,
                (Some(ARTIST_IMAGES_NAMESPACE), ..) => FileRow::ArtistImage,
                (Some(AVATARS_NAMESPACE), ..) => FileRow::Avatar,
                (Some(namespace), release_id, file_name, file_album, cover_album) => {
                    return Err(DbError::unreadable(
                        "coven_files.namespace with the release file or cover naming the file",
                        (namespace, release_id, file_name, file_album, cover_album),
                    ))
                }
            },
        ))
    }
}

#[cfg(test)]
#[path = "file_row_tests.rs"]
mod tests;
