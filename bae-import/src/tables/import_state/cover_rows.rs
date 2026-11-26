//! The candidate's cover choice: its row in `import_candidate_cover`, which
//! holds the bytes a remote one was fetched with, and the copies a catalog
//! serves of a remote one in `import_candidate_cover_copy`.
//! Its tables are `cover_rows.sql`, which every device's library holds.

use super::*;
use crate::cover_art::PreparedCover;
use crate::tables::downscaled_copies::{copies_json_sql, copies_of};
use crate::CoverSelection;
use bae_catalogs::remote_image::RemoteImageSet;
use bae_util::content_type::ContentType;

const COVER_COLUMNS: &str = "content_hash, kind, file_id, url, source";

/// One cover selection as its row states it: the kind, and whichever of the
/// file it names or the address and catalog it names that kind carries.
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

/// Store `cover` as the candidate's: its row, with the bytes a remote one was
/// fetched with, and the copies a catalog serves of a remote one. Every caller
/// writes it in the write that created or found the candidate's state row.
pub(super) fn save_cover(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    cover: &PreparedCover,
) -> Result<(), DbError> {
    let selection = cover.selection();
    let (kind, file_id, url, source) = cover_columns(&selection);
    let fetched = match cover {
        PreparedCover::Remote { fetched, .. } => {
            Some((fetched.content_type.as_str(), fetched.bytes.as_slice()))
        }
        PreparedCover::Local(_) | PreparedCover::Embedded(_) => None,
    };
    // The copies reference the previous choice's address, which the upsert
    // replaces, so they go first.
    sql.execute("DELETE FROM import_candidate_cover_copy WHERE content_hash = ?", [content_hash])?;
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_cover ({COVER_COLUMNS}, content_type, bytes) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (content_hash) DO UPDATE SET \
                 kind = excluded.kind, file_id = excluded.file_id, \
                 url = excluded.url, source = excluded.source, \
                 content_type = excluded.content_type, bytes = excluded.bytes"
        ),
        params![
            content_hash,
            kind,
            file_id,
            url,
            source,
            fetched.map(|(content_type, _)| content_type),
            fetched.map(|(_, bytes)| bytes),
        ],
    )?;
    if let PreparedCover::Remote { image, .. } = cover {
        for copy in &image.downscaled {
            sql.execute(
                "INSERT INTO import_candidate_cover_copy \
                     (content_hash, image_url, max_edge, url) \
                 VALUES (?, ?, ?, ?)",
                params![content_hash, image.url, copy.max_edge, copy.url],
            )?;
        }
    }
    Ok(())
}

/// Remove the candidate's cover choice.
pub(super) fn delete_cover(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_cover WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// A candidate's cover choice as columns of a read that joins its row as
/// `cover`, with the copies the catalog serves of a remote one as a JSON
/// array. A candidate with no choice reads `cover_row` as NULL.
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
#[derive(Clone, PartialEq)]
pub(crate) struct StoredCover {
    kind: String,
    file_id: Option<String>,
    url: Option<String>,
    source: Option<String>,
    copies: String,
}

impl StoredCover {
    /// The columns of a cover a pane's read selects.
    pub(crate) fn new(
        kind: String,
        file_id: Option<String>,
        url: Option<String>,
        source: Option<String>,
        copies: String,
    ) -> Self {
        Self { kind, file_id, url, source, copies }
    }
}

/// The [`cover_read_columns`] of `row`; none for a candidate that chose no
/// cover.
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

impl StoredCover {
    pub(crate) fn selection(self) -> Result<CoverSelection, DbError> {
        // A kind whose column is NULL: the table's CHECK keeps each kind's
        // columns filled, so it is a row nothing here writes.
        let absent = |column: &str| {
            DbError::unreadable(
                format!("import_candidate_cover.{column} of a {} cover", self.kind),
                None::<String>,
            )
        };
        match self.kind.as_str() {
            "local" => Ok(CoverSelection::Local(self.file_id.ok_or_else(|| absent("file_id"))?)),
            "embedded" => {
                Ok(CoverSelection::Embedded(self.file_id.ok_or_else(|| absent("file_id"))?))
            }
            "remote" => Ok(CoverSelection::Remote(
                RemoteImageSet::stored(
                    self.url.ok_or_else(|| absent("url"))?,
                    copies_of(&self.copies)?,
                ),
                {
                    let source = self.source.ok_or_else(|| absent("source"))?;
                    Catalog::from_str(&source).map_err(|_unknown| {
                        DbError::unreadable("import_candidate_cover.source", &source)
                    })?
                },
            )),
            other => Err(DbError::unreadable("import_candidate_cover.kind", other)),
        }
    }
}

// The cover each candidate chose, with the copies the catalog serves of a
// remote one as a JSON array, and the bytes a remote one was fetched with.
sqlive_compose::fragment!(
    ChosenCover {
        prepared_copies: String,
        prepared_content_type: Option<String>,
        prepared_bytes: Option<Vec<u8>>
    },
    r#"
    SELECT cover.content_hash AS prepared_hash, cover.kind AS prepared_kind,
        cover.file_id AS prepared_file_id, cover.url AS prepared_url,
        cover.source AS prepared_source,
        (SELECT json_group_array(json_object('url', copy.url, 'max_edge', copy.max_edge)
                    ORDER BY copy.max_edge)
         FROM import_candidate_cover_copy AS copy
         WHERE copy.content_hash = cover.content_hash AND copy.image_url = cover.url)
            AS prepared_copies,
        cover.content_type AS prepared_content_type, cover.bytes AS prepared_bytes
    FROM import_candidate_cover AS cover
"#
);

impl ChosenCover {
    /// The cover the row chose, with the bytes stored for it: a remote one
    /// with the image its write fetched, which its row holds, a folder's
    /// image as it is.
    pub(super) fn prepared(self) -> Result<PreparedCover, DbError> {
        let ChosenCover {
            prepared_kind,
            prepared_file_id,
            prepared_url,
            prepared_source,
            prepared_copies,
            prepared_content_type,
            prepared_bytes,
            ..
        } = self;
        let selection = StoredCover::new(
            prepared_kind,
            prepared_file_id,
            prepared_url,
            prepared_source,
            prepared_copies,
        )
        .selection()?;
        Ok(match selection {
            CoverSelection::Local(file_id) => PreparedCover::Local(file_id),
            CoverSelection::Embedded(source_file_id) => PreparedCover::Embedded(source_file_id),
            CoverSelection::Remote(image, source) => {
                // The table's CHECK keeps a remote row's bytes filled, so one
                // without them is a row nothing here writes.
                let (Some(content_type), Some(bytes)) = (prepared_content_type, prepared_bytes)
                else {
                    return Err(DbError::unreadable(
                        "the fetched bytes of a remote import_candidate_cover",
                        image.url,
                    ));
                };
                let fetched = bae_catalogs::remote_image::RemoteImage {
                    content_type: ContentType::from_mime(&content_type),
                    bytes,
                };
                PreparedCover::Remote { image, source, fetched }
            }
        })
    }
}

impl ImportTables {
    /// Plant damage for a test: a local cover for `content_hash` naming
    /// `file_id`, a file the scan did not store. No write stores such a row;
    /// a damaged store holds one, and the cover reads check for it.
    #[cfg(test)]
    pub(crate) async fn damage_cover_file_for_test(
        &self,
        content_hash: &str,
        file_id: &str,
    ) -> Result<(), DbError> {
        let (content_hash, file_id) = (content_hash.to_string(), file_id.to_string());
        self.store
            .call(move |sql| {
                sql.execute(
                    "INSERT INTO import_candidate_cover (content_hash, kind, file_id) \
                     VALUES (?, 'local', ?)",
                    params![content_hash, file_id],
                )?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "cover_rows_tests.rs"]
mod tests;
