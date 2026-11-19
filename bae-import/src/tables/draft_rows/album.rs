//! The album a draft files under, as each owner's statement returns it: a
//! candidate's pane and a release editor's draft read it the same way.

use crate::{AlbumLine, DraftAlbum};
use bae_store::DbError;

/// The album a draft files under as one statement returns it: which choice
/// names it (`none`, `new`, `named` or `album`), and the library album that
/// choice names, with its title and its credits as JSON.
pub(crate) struct DraftAlbumRow {
    kind: String,
    album_id: Option<String>,
    title: Option<String>,
    credits: String,
}

impl DraftAlbumRow {
    /// The row a statement returns as its four columns, in that order.
    pub(crate) fn of_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            kind: row.get(0)?,
            album_id: row.get(1)?,
            title: row.get(2)?,
            credits: row.get(3)?,
        })
    }

    /// The album the row names. `owner` names whose draft it is, for the
    /// error of a row bae can't read.
    pub(crate) fn album(self, owner: &str) -> Result<DraftAlbum, DbError> {
        let Self { kind, album_id, title, credits } = self;
        let library = |choice: &str| match (album_id, title) {
            (Some(album_id), Some(title)) => Ok((
                album_id,
                AlbumLine::of(title, bae_records::records::read::credit_line_of_json(&credits)?),
            )),
            _ => Err(DbError::unreadable(format!("the {choice} album of {owner}"), None::<String>)),
        };
        Ok(match kind.as_str() {
            // The person picked nothing and the records name no album.
            "none" => DraftAlbum::New,
            "new" => DraftAlbum::PickedNew,
            "named" => {
                let (album_id, line) = library("named")?;
                DraftAlbum::Named { album_id, line }
            }
            "album" => {
                let (album_id, line) = library("picked")?;
                DraftAlbum::Picked { album_id, line }
            }
            other => return Err(DbError::unreadable(format!("the album kind of {owner}"), other)),
        })
    }
}
