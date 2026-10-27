//! The artist picker's search: every stored artist a typed text finds, in the
//! windows the picker shows, read live so a write to an artist it found, or one
//! that makes another match, reads it again.

use super::*;

/// Every requested window of an artist search, with the found artists' other
/// printed names and image versions, and how many artists it found.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtistSearchProjection {
    pub windows: Vec<crate::library::LibraryBrowseWindow<DbArtist>>,
    /// The other printed names of each artist the windows hold, by artist id.
    pub credit_names: HashMap<String, Vec<crate::credits::ArtistCreditName>>,
    pub image_versions: HashMap<String, String>,
    pub total_count: u64,
}

impl Database {
    /// Search every stored artist, including artists without an album link,
    /// reading `window` of what it finds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn search_artists(
        &self,
        query: &crate::library::LibrarySearchQuery,
        window: crate::library::LibraryPageWindow,
    ) -> Result<Vec<DbArtist>, DbError> {
        let query = query.clone();
        self.read(move |sql| artist_search_rows_on(&sql, &query, &window))
            .await
    }

    /// Follow what an artist search finds, in the windows it asks for. No
    /// query finds nothing; each window it asks for answers with no rows. A
    /// new query or new windows move this subscription's request rather than
    /// opening another.
    pub(crate) fn subscribe_artist_search(
        &self,
        initial: crate::library::ArtistSearchView,
    ) -> coven::ReconfigurableLiveQuery<crate::library::ArtistSearchView, ArtistSearchProjection>
    {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |view, sql| {
                let Some(query) = &view.query else {
                    return Ok(ArtistSearchProjection {
                        windows: view
                            .windows
                            .iter()
                            .map(|window| crate::library::LibraryBrowseWindow {
                                window: window.clone(),
                                rows: Vec::new(),
                            })
                            .collect(),
                        credit_names: HashMap::new(),
                        image_versions: HashMap::new(),
                        total_count: 0,
                    });
                };
                let total_count = artist_search_count_on(&sql, query).map_err(CovenError::from)?;
                let windows = view
                    .windows
                    .iter()
                    .map(|window| {
                        Ok(crate::library::LibraryBrowseWindow {
                            window: window.clone(),
                            rows: artist_search_rows_on(&sql, query, window)?,
                        })
                    })
                    .collect::<Result<Vec<_>, DbError>>()
                    .map_err(CovenError::from)?;
                // Only the artists the windows hold are shown, so only their
                // names and images are read and followed.
                let artist_ids = windows
                    .iter()
                    .flat_map(|window| window.rows.iter().map(|artist| artist.id.clone()))
                    .collect::<Vec<_>>();
                let credit_names =
                    super::artist_credit_names::credit_names_of_on(&sql, &artist_ids)
                        .map_err(CovenError::from)?;
                let image_versions =
                    super::files::image_versions_on(&sql, LibraryImageType::Artist, &artist_ids)
                        .map_err(CovenError::from)?;
                Ok(ArtistSearchProjection {
                    windows,
                    credit_names,
                    image_versions,
                    total_count,
                })
            })
    }
}

/// The artists `?1` finds: by library or provider id, or by a name, sort name
/// or other printed name holding it. Merged-away artists are never found; a
/// credit name finds the artist it shows as.
fn artist_search_filter() -> String {
    format!(
        r#"
        {shown}
          AND (id = ?1
           OR discogs_artist_id = ?1
           OR musicbrainz_artist_id = ?1
           OR instr(lower(name), lower(?1)) > 0
           OR instr(lower(COALESCE(sort_name, '')), lower(?1)) > 0
           OR id IN (
               SELECT {credit_artist} FROM artist_credits credit_name
               WHERE instr(lower(credit_name.name), lower(?1)) > 0))
        "#,
        shown = artist_is_shown("artists"),
        credit_artist = shown_artist_id("credit_name.artist_id"),
    )
}

fn artist_search_count_on(
    sql: &SqlReadContext<'_>,
    query: &crate::library::LibrarySearchQuery,
) -> Result<u64, DbError> {
    sql.query_row(
        &format!(
            "SELECT COUNT(*) FROM artists WHERE {filter}",
            filter = artist_search_filter()
        ),
        params![query.as_str()],
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
    query: &crate::library::LibrarySearchQuery,
    window: &crate::library::LibraryPageWindow,
) -> Result<Vec<DbArtist>, DbError> {
    sql.query(
        &format!(
            r#"
            SELECT *
            FROM artists
            WHERE {filter}
            ORDER BY
                CASE
                    WHEN id = ?1
                      OR discogs_artist_id = ?1
                      OR musicbrainz_artist_id = ?1
                      OR lower(name) = lower(?1)
                      OR lower(COALESCE(sort_name, '')) = lower(?1) THEN 0
                    WHEN substr(lower(name), 1, length(?1)) = lower(?1)
                      OR substr(lower(COALESCE(sort_name, '')), 1, length(?1)) = lower(?1) THEN 1
                    ELSE 2
                END,
                name COLLATE NOCASE,
                id
            LIMIT ?2 OFFSET ?3
            "#,
            filter = artist_search_filter()
        ),
        params![query.as_str(), window.limit as i64, window.offset as i64],
        row_to_artist,
    )
    .map_err(DbError::from)
}
