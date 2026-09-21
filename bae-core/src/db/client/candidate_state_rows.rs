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
        CoverSelection::Remote(url, source) => {
            ("remote", None, Some(url.as_str()), Some(source.as_str()))
        }
    }
}

pub(super) fn save_cover(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    cover: &CoverSelection,
) -> Result<(), DbError> {
    require_state_row(sql, content_hash, "cover choice")?;
    let (kind, file_id, url, source) = cover_columns(cover);
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

impl Database {
    /// Fill in the cover each candidate's folder gives it, for the candidates
    /// scanned before a scan stored one.
    ///
    /// A candidate's cover is a stored value: a scan writes the folder's own
    /// cover the moment the candidate is written, and every reader reads that
    /// row. A candidate scanned before that has no row, and it showed its
    /// folder's artwork only because each reader re-derived it — so the same
    /// rule runs here once, and no candidate loses the artwork it was
    /// showing. A candidate that already has a selection is left alone.
    pub(crate) fn fill_candidate_folder_covers(
        sql: &coven::MigrationContext<'_>,
    ) -> Result<(), DbError> {
        let candidates = sql.query(
            "SELECT candidate.watched_folder_path, candidate.path, candidate.content_hash, \
                    tags.embedded_cover_source_relative_path, tags.embedded_cover_content_type \
             FROM scan_candidate AS candidate \
             JOIN import_candidate_state AS state \
               ON state.content_hash = candidate.content_hash \
             LEFT JOIN scan_candidate_tag_snapshot AS tags \
               ON tags.watched_folder_path = candidate.watched_folder_path \
              AND tags.candidate_path = candidate.path \
             WHERE NOT EXISTS ( \
                 SELECT 1 FROM import_candidate_cover AS cover \
                 WHERE cover.content_hash = candidate.content_hash) \
             ORDER BY candidate.content_hash, candidate.watched_folder_path, candidate.path",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )?;
        // One row per content hash: two scanned folders holding the same files
        // are one candidate state, and its cover is written once.
        let mut written: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (root, path, content_hash, embedded_path, embedded_content_type) in candidates {
            if !written.insert(content_hash.clone()) {
                continue;
            }
            let embedded = match (embedded_path, embedded_content_type) {
                (Some(source), Some(content_type)) => {
                    crate::import::file_tag_snapshot::embedded_cover_of(
                        &source,
                        &crate::util::content_type::ContentType::from_mime(&content_type),
                    )
                }
                _ => None,
            };
            let files = super::folder_scans::read::load_files(sql, &root, Some(&path))?
                .remove(&path)
                .ok_or_else(|| {
                    DbError::Message(format!("candidate {path} has no scanned files"))
                })?;
            let files = crate::import::folder_scanner::CategorizedFiles { files };
            let Some(cover) = crate::import::local_artwork::folder_cover(embedded, files.artwork())
            else {
                continue;
            };
            let (kind, file_id, url, source) = cover_columns(&cover);
            sql.execute(
                &format!(
                    "INSERT INTO import_candidate_cover ({COVER_COLUMNS}) VALUES (?, ?, ?, ?, ?)"
                ),
                params![content_hash, kind, file_id, url, source],
            )?;
        }
        Ok(())
    }
}
