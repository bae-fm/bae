//! Writes shared by candidate discovery and the import pane. A candidate's
//! anchor row must exist before either owner can attach state to it.

use super::*;
use crate::cover_art::PreparedCover;
use crate::CoverSelection;

pub(super) const COVER_COLUMNS: &str = "content_hash, kind, file_id, url, source";

pub(super) fn require_state_row(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    what: &str,
) -> Result<(), DbError> {
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
    Err(DbError::Message(format!("the {what} for {content_hash} has no candidate state row")))
}

/// One cover selection as its row states it: the kind, and whichever of the
/// file it names or the address and catalog it names that kind carries.
pub(super) fn cover_columns(
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
/// a remote one, and the bytes a remote one was fetched with, which are
/// written with the row they belong to or removed with it.
pub(crate) fn save_cover(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    cover: &PreparedCover,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "cover choice")?;
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
