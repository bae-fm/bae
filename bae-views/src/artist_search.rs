//! The artist picker's live search: one query for as long as the picker is
//! open, moved in place to each text the person types and to the windows of
//! its list on screen.

use super::search::LibrarySearchQuery;
use super::*;
use crate::album_detail::ArtistSearchResult;
use bae_db::client::artist_credit_names::{credit_names_column, credit_names_json_sql};
use bae_store::live_query::LiveQueryError;
use bae_store::{LibraryBrowseWindow, LibraryPageWindows};

/// What an artist picker reads: the artists its text finds, parsed as every
/// library search parses a query (`None` is no search), in the windows of its
/// list on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistSearchView {
    pub query: Option<LibrarySearchQuery>,
    pub windows: LibraryPageWindows,
}

/// One value an artist search delivered: each window it was asked for, with
/// the artists that landed in it, how many artists the text found, and the
/// revision of the request it answers.
#[derive(Debug, Clone)]
pub struct ArtistSearchSnapshot {
    pub windows: Vec<LibraryBrowseWindow<ArtistSearchResult>>,
    pub total_count: u64,
    pub request_revision: u64,
}

/// A live artist search whose text and windows change in place.
pub struct LiveArtistSearch {
    query: bae_store::live_query::CancellableLiveQuery<
        ArtistSearchView,
        crate::ArtistSearchProjection,
    >,
    /// The snapshot a projection read for a request revision resolves to.
    resolve: std::sync::Arc<
        dyn Fn(crate::ArtistSearchProjection, u64) -> ArtistSearchSnapshot + Send + Sync,
    >,
}

impl LiveArtistSearch {
    pub fn new(
        query: bae_store::live_query::CancellableLiveQuery<
            ArtistSearchView,
            crate::ArtistSearchProjection,
        >,
        resolve: impl Fn(crate::ArtistSearchProjection, u64) -> ArtistSearchSnapshot
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self { query, resolve: std::sync::Arc::new(resolve) }
    }

    /// Read `windows` of what `raw` finds from now on; a blank text is no
    /// search. Returns the revision the answer will carry, which repeating the
    /// standing request keeps.
    pub fn set_view(&self, raw: &str, windows: LibraryPageWindows) -> Result<u64, LiveQueryError> {
        self.query
            .set(ArtistSearchView { query: LibrarySearchQuery::parse(raw), windows })
            .map_err(LiveQueryError::from)
    }

    pub async fn next(&self) -> Result<ArtistSearchSnapshot, LiveQueryError> {
        let event = self.query.next().await.map_err(LiveQueryError::from)?;
        let request_revision = event.revision.0;
        Ok((self.resolve)(event.result?, request_revision))
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}

/// Every requested window of an artist search, and how many artists it
/// found.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtistSearchProjection {
    pub windows: Vec<bae_store::LibraryBrowseWindow<DbArtistSearchRow>>,
    pub total_count: u64,
}

/// One artist an artist search found, with its image's version and its other
/// printed names in name order, read in the artist's own row.
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtistSearchRow {
    pub artist: DbArtist,
    pub image_version: Option<String>,
    pub credit_names: Vec<bae_db::credits::ArtistCreditName>,
}

impl LibraryViews {
    /// Search every stored artist, including artists without an album link,
    /// reading `window` of what it finds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn search_artists(
        &self,
        query: &crate::LibrarySearchQuery,
        window: bae_store::LibraryPageWindow,
    ) -> Result<Vec<DbArtist>, DbError> {
        let query = query.clone();
        self.store
            .read(move |sql| {
                Ok(artist_search_rows_on(&sql, &query, &window)?
                    .into_iter()
                    .map(|row| row.artist)
                    .collect())
            })
            .await
    }

    /// Follow what an artist search finds, in the windows it asks for. No
    /// query finds nothing; each window it asks for answers with no rows. A
    /// new query or new windows move this subscription's request rather than
    /// opening another.
    pub fn subscribe_artist_search(
        &self,
        initial: crate::ArtistSearchView,
    ) -> coven::ReconfigurableLiveQuery<crate::ArtistSearchView, ArtistSearchProjection> {
        self.store.subscribe_reconfigurable(initial, |view, sql| {
            let Some(query) = &view.query else {
                return Ok(ArtistSearchProjection {
                    windows: view
                        .windows
                        .iter()
                        .map(|window| bae_store::LibraryBrowseWindow {
                            window: window.clone(),
                            rows: Vec::new(),
                        })
                        .collect(),
                    total_count: 0,
                });
            };
            let total_count = artist_search_count_on(&sql, query).map_err(CovenError::from)?;
            let windows = view
                .windows
                .iter()
                .map(|window| {
                    Ok(bae_store::LibraryBrowseWindow {
                        window: window.clone(),
                        rows: artist_search_rows_on(&sql, query, window)?,
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()
                .map_err(CovenError::from)?;
            Ok(ArtistSearchProjection { windows, total_count })
        })
    }
}

/// The artists an artist search's windows hold, each beside its image
/// and its other printed names.
pub fn resolve_artist_search(
    projection: ArtistSearchProjection,
    request_revision: u64,
) -> crate::ArtistSearchSnapshot {
    let ArtistSearchProjection { windows, total_count } = projection;
    crate::ArtistSearchSnapshot {
        windows: windows
            .into_iter()
            .map(|window| bae_store::LibraryBrowseWindow {
                window: window.window,
                rows: window
                    .rows
                    .into_iter()
                    .map(|row| crate::album_detail::ArtistSearchResult {
                        image: bae_db::artist_image_ref(&row.artist.id, row.image_version),
                        credit_names: row.credit_names,
                        artist: row.artist,
                    })
                    .collect(),
            })
            .collect(),
        total_count,
        request_revision,
    }
}

/// The artists the text `?1` finds: by library or provider id, or as
/// library search finds artists, by the search pattern `pattern` binds
/// ([`artists_found_sql`]). Merged-away artists are never found.
fn artist_search_filter(pattern: &str) -> String {
    format!(
        r#"
        {shown}
          AND (id = ?1
           OR discogs_artist_id = ?1
           OR musicbrainz_artist_id = ?1
           OR id IN ({found}))
        "#,
        shown = artist_is_shown("artists"),
        found = artists_found_sql(pattern),
    )
}

fn artist_search_count_on(
    sql: &SqlReadContext<'_>,
    query: &crate::LibrarySearchQuery,
) -> Result<u64, DbError> {
    sql.query_row(
        &format!(
            "SELECT COUNT(*) FROM artists WHERE {filter}",
            filter = artist_search_filter("?2")
        ),
        params![query.as_str(), search_pattern(query.as_str())],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as u64)
    .map_err(DbError::from)
}

/// One window of what `query` finds. Library and provider IDs join exact name
/// or sort-name matches ahead of prefixes, then substrings; name and library
/// ID stabilize each rank, so windows read at once never overlap or skip.
fn artist_search_rows_on(
    sql: &SqlReadContext<'_>,
    query: &crate::LibrarySearchQuery,
    window: &bae_store::LibraryPageWindow,
) -> Result<Vec<DbArtistSearchRow>, DbError> {
    sql.query(
        &format!(
            r#"
            SELECT *,
                   {image} AS image_version,
                   {credit_names} AS credit_names_json
            FROM artists
            WHERE {filter}
            ORDER BY
                CASE
                    WHEN id = ?1
                      OR discogs_artist_id = ?1
                      OR musicbrainz_artist_id = ?1
                      OR bae_fold_case(name) = bae_fold_case(?1)
                      OR bae_fold_case(sort_name) = bae_fold_case(?1) THEN 0
                    WHEN substr(bae_fold_case(name), 1, length(bae_fold_case(?1)))
                           = bae_fold_case(?1)
                      OR substr(bae_fold_case(sort_name), 1, length(bae_fold_case(?1)))
                           = bae_fold_case(?1) THEN 1
                    ELSE 2
                END,
                name COLLATE NOCASE,
                id
            LIMIT ?2 OFFSET ?3
            "#,
            filter = artist_search_filter("?4"),
            image = artist_image_version_sql("artists.id"),
            credit_names = credit_names_json_sql("artists.id"),
        ),
        params![
            query.as_str(),
            window.limit as i64,
            window.offset as i64,
            search_pattern(query.as_str())
        ],
        |row| {
            Ok(DbArtistSearchRow {
                artist: row_to_artist(row)?,
                image_version: row.get("image_version")?,
                credit_names: credit_names_column(row, "credit_names_json")?,
            })
        },
    )
    .map_err(DbError::from)
}

#[cfg(test)]
#[path = "artist_search_tests.rs"]
mod tests;
