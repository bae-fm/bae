//! The candidate's cover choice: its row in `import_candidate_cover`, the
//! copies a catalog serves of a remote one in `import_candidate_cover_copy`,
//! and the bytes a remote one was fetched with in
//! `import_candidate_remote_cover_asset`, which are written with the row they
//! belong to or removed with it.
//! Its tables are `cover_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("cover_rows.sql");

#[cfg(desktop)]
use super::*;
#[cfg(desktop)]
use crate::cover_art::PreparedCover;
#[cfg(desktop)]
use crate::tables::downscaled_copies::{copies_json_sql, copies_of};
#[cfg(desktop)]
use crate::CoverSelection;
#[cfg(desktop)]
use bae_catalogs::remote_image::RemoteImageSet;
#[cfg(desktop)]
use bae_util::content_type::ContentType;

#[cfg(desktop)]
const COVER_COLUMNS: &str = "content_hash, kind, file_id, url, source";

/// Refuse a cover for a candidate with no state row: the row it hangs off.
#[cfg(desktop)]
fn require_state_row(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    let present = sql
        .query_row(
            "SELECT 1 FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if present {
        return Ok(());
    }
    Err(DbError::Message(format!("the cover choice for {content_hash} has no candidate state row")))
}

/// One cover selection as its row states it: the kind, and whichever of the
/// file it names or the address and catalog it names that kind carries.
#[cfg(desktop)]
fn cover_columns(
    cover: &CoverSelection,
) -> (&'static str, Option<&str>, Option<&str>, Option<&str>) {
    match cover {
        CoverSelection::Local(file_id) => ("local", Some(file_id.as_str()), None, None),
        CoverSelection::Embedded(source_file_id) => {
            ("embedded", Some(source_file_id.as_str()), None, None)
        }
        CoverSelection::Remote(image, source) => {
            ("remote", None, Some(image.url.as_str()), Some(source.as_str()))
        }
    }
}

/// Store `cover` as the candidate's: its row, the copies a catalog serves of
/// a remote one, and the bytes a remote one was fetched with.
#[cfg(desktop)]
pub(super) fn save_cover(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    cover: &PreparedCover,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash)?;
    let selection = cover.selection();
    let (kind, file_id, url, source) = cover_columns(&selection);
    // The copies reference the previous choice's address, which the upsert
    // replaces, so they go first.
    sql.execute("DELETE FROM import_candidate_cover_copy WHERE content_hash = ?", [content_hash])?;
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_cover ({COVER_COLUMNS}) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (content_hash) DO UPDATE SET \
                 kind = excluded.kind, file_id = excluded.file_id, \
                 url = excluded.url, source = excluded.source"
        ),
        params![content_hash, kind, file_id, url, source],
    )?;
    match cover {
        PreparedCover::Remote { image, fetched, .. } => {
            for copy in &image.downscaled {
                sql.execute(
                    "INSERT INTO import_candidate_cover_copy \
                         (content_hash, image_url, max_edge, url) \
                     VALUES (?, ?, ?, ?)",
                    params![content_hash, image.url, copy.max_edge, copy.url],
                )?;
            }
            sql.execute(
                "INSERT INTO import_candidate_remote_cover_asset \
                     (content_hash, content_type, bytes) VALUES (?, ?, ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     content_type = excluded.content_type, bytes = excluded.bytes",
                params![content_hash, fetched.content_type.as_str(), fetched.bytes],
            )?;
        }
        PreparedCover::Local(_) | PreparedCover::Embedded(_) => {
            sql.execute(
                "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
                [content_hash],
            )?;
        }
    }
    Ok(())
}

/// Remove the candidate's cover choice.
#[cfg(desktop)]
pub(super) fn delete_cover(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_cover WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// A candidate's cover choice as columns of a read that joins its row as
/// `cover`, with the copies the catalog serves of a remote one as a JSON
/// array. A candidate with no choice reads `cover_row` as NULL.
#[cfg(desktop)]
pub(crate) fn cover_read_columns() -> String {
    // Only a remote choice has an address for copies to reference.
    let copies = copies_json_sql(
        "import_candidate_cover_copy",
        "copy.content_hash = cover.content_hash AND copy.image_url = cover.url",
    );
    format!(
        "cover.content_hash AS cover_row, cover.kind AS cover_kind, \
         cover.file_id AS cover_file_id, cover.url AS cover_url, \
         cover.source AS cover_source, {copies} AS cover_copies"
    )
}

/// One candidate's [`cover_read_columns`] as SQLite hands them over.
#[cfg(desktop)]
pub(crate) struct StoredCover {
    kind: String,
    file_id: Option<String>,
    url: Option<String>,
    source: Option<String>,
    copies: String,
}

/// The [`cover_read_columns`] of `row`; none for a candidate that chose no
/// cover.
#[cfg(desktop)]
pub(crate) fn read_cover(row: &Row<'_>) -> rusqlite::Result<Option<StoredCover>> {
    if row.get::<_, Option<String>>("cover_row")?.is_none() {
        return Ok(None);
    }
    Ok(Some(StoredCover {
        kind: row.get("cover_kind")?,
        file_id: row.get("cover_file_id")?,
        url: row.get("cover_url")?,
        source: row.get("cover_source")?,
        copies: row.get("cover_copies")?,
    }))
}

#[cfg(desktop)]
impl StoredCover {
    pub(crate) fn selection(self) -> Result<CoverSelection, DbError> {
        match self.kind.as_str() {
            "local" => Ok(CoverSelection::Local(
                self.file_id
                    .ok_or_else(|| DbError::Message("a local cover names no file".into()))?,
            )),
            "embedded" => Ok(CoverSelection::Embedded(self.file_id.ok_or_else(|| {
                DbError::Message("an embedded cover names no source file".into())
            })?)),
            "remote" => {
                Ok(CoverSelection::Remote(
                    RemoteImageSet::with_copies(
                        self.url.ok_or_else(|| {
                            DbError::Message("a remote cover names no address".into())
                        })?,
                        copies_of(&self.copies)?,
                    ),
                    Catalog::from_str(&self.source.ok_or_else(|| {
                        DbError::Message("a remote cover names no source".into())
                    })?)
                    .map_err(DbError::Message)?,
                ))
            }
            other => Err(DbError::unreadable("import_candidate_cover.kind", other)),
        }
    }
}

/// The cover the candidate `content_hash` chose, if it chose one.
#[cfg(desktop)]
pub(super) fn load_cover_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    content_hash: &str,
) -> Result<Option<CoverSelection>, DbError> {
    sql.query_row(
        &format!(
            "SELECT {} FROM import_candidate_cover AS cover WHERE cover.content_hash = ?",
            cover_read_columns()
        ),
        [content_hash],
        read_cover,
    )
    .optional()?
    .flatten()
    .map(StoredCover::selection)
    .transpose()
}

/// The candidate's cover `selection` with the bytes stored for it: a remote
/// selection with the image its write fetched, a folder's image as it is.
/// The cover's save writes the two together, so a remote cover stored
/// without its bytes, or bytes stored beside a cover of the folder, is a
/// damaged row, and the read fails naming it.
#[cfg(desktop)]
pub(super) fn load_prepared_cover_on(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    content_hash: &str,
    selection: Option<CoverSelection>,
) -> Result<Option<PreparedCover>, DbError> {
    let fetched = sql
        .query_row(
            "SELECT content_type, bytes FROM import_candidate_remote_cover_asset \
             WHERE content_hash = ?",
            [content_hash],
            |row| {
                Ok(bae_catalogs::remote_image::RemoteImage {
                    content_type: ContentType::from_mime(&row.get::<_, String>(0)?),
                    bytes: row.get(1)?,
                })
            },
        )
        .optional()?;
    match (selection, fetched) {
        (Some(CoverSelection::Remote(image, source)), Some(fetched)) => {
            Ok(Some(PreparedCover::Remote { image, source, fetched }))
        }
        (Some(CoverSelection::Remote(..)), None) => Err(DbError::Message(format!(
            "candidate {content_hash}'s remote cover has no stored bytes"
        ))),
        (Some(CoverSelection::Local(_) | CoverSelection::Embedded(_)) | None, Some(_)) => {
            Err(DbError::Message(format!(
                "candidate {content_hash} stores remote-cover bytes without a remote cover"
            )))
        }
        (Some(CoverSelection::Local(file_id)), None) => Ok(Some(PreparedCover::Local(file_id))),
        (Some(CoverSelection::Embedded(source_file_id)), None) => {
            Ok(Some(PreparedCover::Embedded(source_file_id)))
        }
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
#[path = "cover_rows_tests.rs"]
mod tests;
