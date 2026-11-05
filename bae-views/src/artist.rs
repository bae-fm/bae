//! The artist views: the artist list and an artist's detail.
//!
//! Its tables and the triggers keeping them are `artist.sql`.

use super::*;
use crate::album_detail::{AlbumSummary, ArtistDetail, ArtistSummary};

/// The SQL creating this part's tables and triggers.
pub(super) const SQL: &str = include_str!("artist.sql");

impl LibraryViews {
    pub async fn get_artist_count(&self) -> Result<u64, DbError> {
        self.database.read(|sql| artist_count_on(&sql)).await
    }

    pub async fn get_artist_page(
        &self,
        sort: &[ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<ArtistSummary>, DbError> {
        let order = artist_order(sort);
        let rows: Vec<DbArtistSummary> = self
            .database
            .read(move |sql| {
                Ok(artist_rows_on(&sql, &order, &order.everything(), limit, offset)?
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect())
            })
            .await?;
        Ok(rows.into_iter().map(ArtistSummary::from_raw).collect())
    }

    /// Follow the artist list under `sort` in the windows its request
    /// names, each read from the row beside it when one is known
    /// (`list_windows`).
    pub fn subscribe_artist_browse(
        &self,
        sort: &[ArtistSortCriterion],
        initial_windows: bae_db::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<bae_db::LibraryPageWindows, ArtistBrowseProjection> {
        let order = artist_order(sort);
        let places = KnownPlaces::new();
        self.database.subscribe_reconfigurable(initial_windows, move |requested, sql| {
            let versions = list_versions_on(&sql, crate::versions::TABLE, &["artist_listing"])?;
            let total_count = artist_count_on(&sql).map_err(CovenError::from)?;
            let WindowsRead { windows, found } =
                read_windows(&order, requested, places.under(&versions), |seek, limit, offset| {
                    artist_rows_on(&sql, &order, seek, limit, offset)
                })?;
            places.keep(versions, found);
            Ok(ArtistBrowseProjection {
                windows: windows
                    .into_iter()
                    .map(|(window, rows)| bae_db::LibraryBrowseWindow { window, rows })
                    .collect(),
                total_count,
            })
        })
    }

    /// The artist's page as its view shows it. `None` when the artist
    /// doesn't exist or has no album links.
    pub async fn get_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<ArtistDetail>, DbError> {
        Ok(self.find_artist_detail(artist_id).await?.map(artist_detail_of))
    }

    /// The artist's summary row plus every album whose credit list names it,
    /// in discography order: year
    /// ascending with unknown years last, then title (case-insensitive), then
    /// id. `None` when the artist doesn't exist or has no album links —
    /// exactly the artists `get_artist_page` never lists.
    pub(crate) async fn find_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtistDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.database
            .read(move |sql| find_artist_detail_on(&sql, &artist_id))
            .process(|rows| rows.map(ArtistDetailRows::process).transpose())
            .await
    }

    /// Follow the artist `initial` names, or none. The detail view moves to
    /// another artist through the request handle, not another query.
    pub fn subscribe_artist_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ArtistDetailProjection> {
        self.database
            .subscribe_reconfigurable(initial, |artist_id, sql| {
                let Some(artist_id) = artist_id else {
                    return Ok(None);
                };
                find_artist_detail_on(&sql, artist_id).map_err(CovenError::from)
            })
            .process(|detail| {
                Ok(ArtistDetailProjection {
                    detail: detail.map(ArtistDetailRows::process).transpose()?,
                })
            })
    }
}

/// One delivery of the artist list as the list shows it.
pub fn resolve_artist_browse(
    projection: ArtistBrowseProjection,
    request_revision: u64,
    cause: coven::LiveQueryCause,
) -> crate::LibraryBrowseSnapshot<ArtistSummary> {
    crate::LibraryBrowseSnapshot {
        windows: projection
            .windows
            .into_iter()
            .map(|window| bae_db::LibraryBrowseWindow {
                window: window.window,
                rows: window.rows.into_iter().map(ArtistSummary::from_raw).collect(),
            })
            .collect(),
        total_count: projection.total_count,
        request_revision,
        cause,
    }
}

/// One delivery of the artist detail as the detail view shows it.
pub fn resolve_artist_detail_projection(
    projection: ArtistDetailProjection,
) -> Option<ArtistDetail> {
    projection.detail.map(artist_detail_of)
}

/// An artist's page as its rows read it.
fn artist_detail_of(raw: DbArtistDetail) -> ArtistDetail {
    ArtistDetail {
        artist: ArtistSummary::from_raw(raw.artist),
        credit_names: raw.credit_names,
        albums: raw.albums.into_iter().map(AlbumSummary::from_raw).collect(),
    }
}

fn find_artist_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<ArtistDetailRows>, DbError> {
    let artist = sql
        .query_row(
            &artist_listing_query(None, Some("WHERE listing.artist_id = ?"), None),
            params![artist_id],
            row_to_artist_summary,
        )
        .optional()?;
    let Some(artist) = artist else {
        return Ok(None);
    };
    let albums_query = format!(
        "{select} \
         FROM album_artist_link link \
         JOIN albums a ON a.id = link.album_id \
         WHERE link.artist_id = ?1 \
         ORDER BY CASE WHEN a.year IS NULL THEN 1 ELSE 0 END, \
                  a.year, a.title COLLATE NOCASE, a.id",
        select = album_summary_select(),
    );
    let albums = sql.query(&albums_query, params![artist_id], AlbumSummaryRow::read)?;
    let credit_names = bae_db::client::artist_credit_names::credit_names_of_on(
        sql,
        std::slice::from_ref(&artist.artist.id),
    )?
    .remove(&artist.artist.id)
    .unwrap_or_default();
    Ok(Some(ArtistDetailRows { artist, credit_names, albums }))
}

#[derive(Clone, PartialEq)]
struct ArtistDetailRows {
    artist: DbArtistSummary,
    credit_names: Vec<bae_db::credits::ArtistCreditName>,
    albums: Vec<AlbumSummaryRow>,
}

impl ArtistDetailRows {
    fn process(self) -> Result<DbArtistDetail, DbError> {
        Ok(DbArtistDetail {
            artist: self.artist,
            credit_names: self.credit_names,
            albums: self
                .albums
                .into_iter()
                .map(AlbumSummaryRow::process)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// The artist list's rows `seek` reads in `order`, at most `limit` from
/// `offset`, each with its key.
pub(crate) fn artist_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbArtistSummary>>, DbError> {
    let statement = listing_window_query(artist_listing_query, order, seek);
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| Ok((row_to_artist_summary(row)?, order.key_of(row)?)))
    })?)
}

fn artist_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM artist_listing", [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(DbError::from)
}

/// The artist list's rows (`artist_listing`), each with its artist as
/// `ar`, as [`row_to_artist_summary`] reads them, and each row's key in
/// `keyed`'s order when one is given; `filter` and `tail` follow the join.
/// An artist is listed for the albums whose credit lists (`album_artists`)
/// show it; track credits (`track_artists`) don't list it. Various Artists
/// gets no special case: it is a real `artists` row and lists like any
/// other artist.
pub(crate) fn artist_listing_query(
    keyed: Option<&ListOrder>,
    filter: Option<&str>,
    tail: Option<&str>,
) -> String {
    let mut query = format!(
        "SELECT ar.id AS artist_id,
                ar.name AS artist_name,
                ar.sort_name AS artist_sort_name,
                ar.discogs_artist_id AS artist_discogs_artist_id,
                ar.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                ar.created_at AS artist_created_at,
                {image} AS artist_image_version,
                listing.album_count{keys}
         FROM artist_listing listing
         JOIN artists ar ON ar.id = listing.artist_id
         ",
        image = artist_image_version_sql("ar.id"),
        keys = key_columns(keyed),
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

pub(crate) fn row_to_artist_summary(row: &Row<'_>) -> rusqlite::Result<DbArtistSummary> {
    let artist = row_to_artist_with_prefix(row, "artist_")?;
    Ok(DbArtistSummary {
        image: bae_db::artist_image_ref(&artist.id, row.get("artist_image_version")?),
        artist,
        album_count: row.get("album_count")?,
    })
}

/// The artist list's order, over `artist_listing listing`: each criterion,
/// then name ascending unless the sort already orders by name, then id, the
/// tie-break a window needs. An index of `artist_listing` serves each
/// one-field sort.
pub(crate) fn artist_order(sort: &[ArtistSortCriterion]) -> ListOrder {
    let terms = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ArtistSortField::Name => "listing.name",
                ArtistSortField::AlbumCount => "listing.album_count",
            };
            SortTerm::running(field, c.direction)
        })
        .collect::<Vec<_>>();
    let ordered_by_name = sort.iter().any(|c| c.field == ArtistSortField::Name);
    ListOrder::named(terms, ordered_by_name, "listing.name", "listing.artist_id")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistBrowseProjection {
    pub windows: Vec<bae_db::LibraryBrowseWindow<DbArtistSummary>>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistDetailProjection {
    pub detail: Option<DbArtistDetail>,
}

#[cfg(test)]
#[path = "artist_tests.rs"]
mod tests;
