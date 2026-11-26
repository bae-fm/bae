//! The album a draft files under, as each owner's statement returns it: a
//! candidate's pane and a release editor's draft read it the same way.

use crate::{AlbumLine, DraftAlbum};
use bae_store::DbError;

/// A library album as a statement returns it: its id, its title and its
/// credits as JSON.
type LibraryAlbumRow = (String, String, String);

/// The album a draft files under as one statement returns it: which choice
/// names it (`none`, `new`, `named` or `album`), and the library album that
/// choice names, when it names one.
pub(crate) struct DraftAlbumRow {
    kind: String,
    library: Option<LibraryAlbumRow>,
}

impl DraftAlbumRow {
    /// The row a statement returns as its four columns, in that order. The
    /// album's three columns are all present or all NULL.
    pub(crate) fn of_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let library = match (row.get(1)?, row.get(2)?, row.get(3)?) {
            (Some(album_id), Some(title), Some(credits)) => Some((album_id, title, credits)),
            (None, None, None) => None,
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Null,
                    "a library album's id, title and credits are not all present".into(),
                ))
            }
        };
        Ok(Self { kind: row.get(0)?, library })
    }

    /// The row a pane's read selects, which
    /// [`crate::tables::import_state::fragments::CandidateAlbum`] names.
    pub(crate) fn of(
        album: crate::tables::import_state::fragments::CandidateAlbum,
    ) -> Result<Self, DbError> {
        let library = album
            .album
            .map(bae_records::records::fragments::LibraryAlbum::into_parts)
            .transpose()?;
        Ok(Self { kind: album.candidate_album_kind, library })
    }

    /// The album the row names. `owner` names whose draft it is, for the
    /// error of a row bae can't read.
    pub(crate) fn album(self, owner: &str) -> Result<DraftAlbum, DbError> {
        let Self { kind, library } = self;
        let named = |choice: &str| {
            let (album_id, title, credits) = library.ok_or_else(|| {
                DbError::unreadable(format!("the {choice} album of {owner}"), None::<String>)
            })?;
            let line =
                AlbumLine::of(title, bae_records::records::read::credit_line_of_json(&credits)?);
            Ok::<_, DbError>((album_id, line))
        };
        Ok(match kind.as_str() {
            // The person picked nothing and the records name no album.
            "none" => DraftAlbum::New,
            "new" => DraftAlbum::PickedNew,
            "named" => {
                let (album_id, line) = named("named")?;
                DraftAlbum::Named { album_id, line }
            }
            "album" => {
                let (album_id, line) = named("picked")?;
                DraftAlbum::Picked { album_id, line }
            }
            other => return Err(DbError::unreadable(format!("the album kind of {owner}"), other)),
        })
    }
}
