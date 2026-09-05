//! The pane's per-candidate state between visits: which surface it shows,
//! the typed-search form, and the last command that failed.

use super::*;
use crate::import::{CandidateSession, MetadataPresentation, SearchForm, SearchTab};

fn presentation_column(presentation: MetadataPresentation) -> &'static str {
    match presentation {
        MetadataPresentation::Draft => "draft",
        MetadataPresentation::FindOnline => "find_online",
        MetadataPresentation::FileTags => "file_tags",
    }
}

fn presentation_of(column: &str) -> Result<MetadataPresentation, DbError> {
    match column {
        "draft" => Ok(MetadataPresentation::Draft),
        "find_online" => Ok(MetadataPresentation::FindOnline),
        "file_tags" => Ok(MetadataPresentation::FileTags),
        other => Err(DbError::Message(format!(
            "unreadable session presentation {other:?}"
        ))),
    }
}

fn tab_column(tab: SearchTab) -> &'static str {
    match tab {
        SearchTab::General => "general",
        SearchTab::CatalogNumber => "catalog_number",
        SearchTab::Barcode => "barcode",
    }
}

fn tab_of(column: &str) -> Result<SearchTab, DbError> {
    match column {
        "general" => Ok(SearchTab::General),
        "catalog_number" => Ok(SearchTab::CatalogNumber),
        "barcode" => Ok(SearchTab::Barcode),
        other => Err(DbError::Message(format!(
            "unreadable session search tab {other:?}"
        ))),
    }
}

/// The session the pane left for `content_hash`, or `None` before it has
/// touched the candidate.
pub(super) fn load_session_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Option<CandidateSession>, DbError> {
    let row = sql
        .query_row(
            "SELECT presentation, search_tab, search_artist, search_album, \
                    search_catalog, search_barcode, error \
             FROM import_candidate_session WHERE content_hash = ?",
            [content_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(presentation, tab, artist, album, catalog, barcode, error)| {
            Ok(CandidateSession {
                presentation: presentation_of(&presentation)?,
                search: SearchForm {
                    tab: tab_of(&tab)?,
                    artist,
                    album,
                    catalog,
                    barcode,
                },
                error,
            })
        },
    )
    .transpose()
}

impl Database {
    /// Record the pane's state for a candidate, whole: the row is the session,
    /// and the caller hands over the next value of all of it.
    pub async fn save_import_candidate_session(
        &self,
        content_hash: &str,
        session: &CandidateSession,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let session = session.clone();
        self.call(move |sql| {
            let affected = sql.execute(
                "INSERT INTO import_candidate_session (\
                     content_hash, presentation, search_tab, search_artist, \
                     search_album, search_catalog, search_barcode, error) \
                 SELECT ?, ?, ?, ?, ?, ?, ?, ? \
                 WHERE EXISTS (SELECT 1 FROM import_candidate_state WHERE content_hash = ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     presentation = excluded.presentation, \
                     search_tab = excluded.search_tab, \
                     search_artist = excluded.search_artist, \
                     search_album = excluded.search_album, \
                     search_catalog = excluded.search_catalog, \
                     search_barcode = excluded.search_barcode, \
                     error = excluded.error",
                params![
                    content_hash,
                    presentation_column(session.presentation),
                    tab_column(session.search.tab),
                    session.search.artist,
                    session.search.album,
                    session.search.catalog,
                    session.search.barcode,
                    session.error,
                    content_hash,
                ],
            )?;
            if affected == 0 {
                return Err(DbError::Message(
                    "the pane's session has no candidate state row to hang off".to_string(),
                ));
            }
            Ok(())
        })
        .await
    }
}
