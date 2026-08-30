//! Writes shared by candidate discovery and the import pane. A candidate's
//! anchor row must exist before either owner can attach state to it.

use super::*;
use crate::import::CoverSelection;

pub(super) const COVER_COLUMNS: &str = "content_hash, kind, file_id, url, source";

pub(super) fn require_state_row(
    sql: &SqlContext<'_, '_>,
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
    Err(DbError::Message(format!(
        "the {what} for {content_hash} has no candidate state row"
    )))
}

pub(super) fn save_cover(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    cover: &CoverSelection,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "cover choice")?;
    let (kind, file_id, url, source) = match cover {
        CoverSelection::Local(file_id) => ("local", Some(file_id.as_str()), None, None),
        CoverSelection::Embedded(source_file_id) => {
            ("embedded", Some(source_file_id.as_str()), None, None)
        }
        CoverSelection::Remote(url, source) => {
            ("remote", None, Some(url.as_str()), Some(source.as_str()))
        }
    };
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_cover ({COVER_COLUMNS}) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (content_hash) DO UPDATE SET \
                 kind = excluded.kind, file_id = excluded.file_id, \
                 url = excluded.url, source = excluded.source"
        ),
        params![content_hash, kind, file_id, url, source],
    )?;
    Ok(())
}
