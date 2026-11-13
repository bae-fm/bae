//! A composer's page: the composer and the work it opens first, and the
//! page's rows (its works, then its unlinked release credits, then its
//! unlinked track credits) read in windows from `composer_detail_row`, which
//! holds them in the page's order.
//!
//! Its tables and the triggers keeping them are `composer_detail.sql`.

use super::*;
use crate::album_detail::{ComposerDetail, ComposerDetailItem, ComposerDetailRow, ComposerHeading};
use crate::composer::work_summary_columns;
use bae_records::records::read::{credit_name_column, credit_name_join};

/// The SQL creating this part's tables and triggers.
pub(super) const SQL: &str = include_str!("composer_detail.sql");

impl LibraryViews {
    #[cfg(test)]
    pub(crate) async fn find_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbComposerDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.store.read(move |sql| find_composer_detail_on(&sql, &artist_id)).await
    }

    /// Follow the composer `initial` names, or none. The detail view moves to
    /// another composer through the request handle, not another query.
    pub fn subscribe_composer_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ComposerDetailProjection, bae_store::DbError>
    {
        self.store.subscribe_reconfigurable(initial, |artist_id, sql| {
            let Some(artist_id) = artist_id else {
                return Ok(ComposerDetailProjection { detail: None });
            };
            Ok(ComposerDetailProjection { detail: find_composer_detail_on(&sql, artist_id)? })
        })
    }

    /// The rows of `artist_id`'s page from `offset`, at most `limit`.
    #[cfg(test)]
    pub(crate) async fn get_composer_detail_rows(
        &self,
        artist_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerDetailRow>, DbError> {
        let artist_id = artist_id.to_string();
        self.store
            .read(move |sql| {
                let order = composer_detail_order("listing");
                Ok(composer_detail_rows_on(
                    &sql,
                    &artist_id,
                    &order,
                    &order.everything(),
                    limit,
                    offset,
                )?
                .into_iter()
                .map(|(row, _)| row)
                .collect())
            })
            .await
    }

    /// Follow the rows of `artist_id`'s page in the windows its request
    /// names, each read from the row beside it when one is known
    /// (`list_windows`), with how many rows the page holds.
    pub fn subscribe_composer_detail_rows(
        &self,
        artist_id: &str,
        initial_windows: bae_store::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        bae_store::LibraryPageWindows,
        ComposerDetailRowsProjection,
        bae_store::DbError,
    > {
        let artist_id = artist_id.to_string();
        let order = composer_detail_order("listing");
        let places = KnownPlaces::new();
        self.store.subscribe_reconfigurable(initial_windows, move |requested, sql| {
            let versions =
                list_versions_on(&sql, crate::versions::TABLE, &["composer_detail_row"])?;
            let total_count = composer_detail_row_count_on(&sql, &artist_id)?;
            let WindowsRead { windows, found } =
                read_windows(&order, requested, places.under(&versions), |seek, limit, offset| {
                    composer_detail_rows_on(&sql, &artist_id, &order, seek, limit, offset)
                })?;
            places.keep(versions, found);
            Ok(ComposerDetailRowsProjection {
                windows: windows
                    .into_iter()
                    .map(|(window, rows)| bae_store::LibraryBrowseWindow { window, rows })
                    .collect(),
                total_count,
            })
        })
    }
}

/// One delivery of a composer's page above its rows, as the detail view
/// shows it.
pub fn resolve_composer_detail_projection(
    projection: ComposerDetailProjection,
) -> Option<ComposerDetail> {
    projection
        .detail
        .map(|raw| ComposerDetail { composer: raw.composer, default_work_id: raw.default_work_id })
}

/// One delivery of a composer's page's rows as the page shows them.
pub fn resolve_composer_detail_rows(
    projection: ComposerDetailRowsProjection,
    request_revision: u64,
    cause: coven::LiveQueryCause,
) -> crate::LibraryBrowseSnapshot<ComposerDetailRow> {
    crate::LibraryBrowseSnapshot {
        windows: projection
            .windows
            .into_iter()
            .map(|window| bae_store::LibraryBrowseWindow {
                window: window.window,
                rows: window.rows.into_iter().map(composer_detail_row_of).collect(),
            })
            .collect(),
        total_count: projection.total_count,
        request_revision,
        cause,
    }
}

/// A row of a composer's page as its read found it, with the heading it
/// opens.
fn composer_detail_row_of(raw: DbComposerDetailRow) -> ComposerDetailRow {
    let DbComposerDetailRow { entry, opens_heading, item } = raw;
    let item = match item {
        DbComposerDetailItem::Work { work, part } => ComposerDetailItem::Work { work, part },
        DbComposerDetailItem::ReleaseCredit(credit) => ComposerDetailItem::ReleaseCredit(credit),
        DbComposerDetailItem::TrackCredit(credit) => ComposerDetailItem::TrackCredit(credit),
    };
    ComposerDetailRow { id: entry, heading: opens_heading.then(|| heading_over(&item)), item }
}

/// Where a composer's page puts its headings: one over each of its parts
/// (`composer_detail_row.section`), opened by the part's first row
/// ([`opens_heading_sql`]), worded by what the part lists.
fn heading_over(item: &ComposerDetailItem) -> ComposerHeading {
    match item {
        ComposerDetailItem::Work { .. } => ComposerHeading::Works,
        ComposerDetailItem::ReleaseCredit(_) => ComposerHeading::Credits,
        ComposerDetailItem::TrackCredit(_) => ComposerHeading::Recordings,
    }
}

/// Whether the row `listing` is the first of its part of the page, which
/// opens the part's heading ([`heading_over`]): one index seek per row.
fn opens_heading_sql() -> String {
    format!(
        "((SELECT first.entry FROM composer_detail_row first
           WHERE first.artist_id = listing.artist_id AND first.section = listing.section
           ORDER BY {} LIMIT 1) = listing.entry)",
        composer_detail_order("first").clause()
    )
}

/// The page's order over `composer_detail_row` aliased `alias`: the order its
/// index (`idx_composer_detail_row_order`) holds after the artist, which ends
/// with terms that tell every row of a page apart.
pub(crate) fn composer_detail_order(alias: &str) -> ListOrder {
    let term = |column: &str| format!("{alias}.{column}");
    ListOrder::new(vec![
        SortTerm::ascending(term("section")),
        SortTerm::ascending(term("group_title_sort_key")),
        SortTerm::ascending(term("group_album")).nullable(),
        SortTerm::ascending(term("group_order")).nullable(),
        SortTerm::ascending(term("group_id")),
        SortTerm::ascending(term("place")).nullable(),
        SortTerm::ascending(term("title_sort_key")).nullable(),
        SortTerm::ascending(term("position")).nullable(),
        SortTerm::ascending(term("item_id")),
    ])
}

/// The composer `artist_id` names, with the first work its page lists;
/// `None` when the composer list doesn't show it.
fn find_composer_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<DbComposerDetail>, DbError> {
    let composer = sql
        .query_row(
            &composer_listing_query(None, Some("WHERE listing.artist_id = ?"), None),
            params![artist_id],
            row_to_composer_summary,
        )
        .optional()?;
    let Some(composer) = composer else {
        return Ok(None);
    };
    let default_work_id = sql
        .query_row(
            &format!(
                "SELECT listing.item_id FROM composer_detail_row listing
                 WHERE listing.artist_id = ? AND listing.section = 0
                 ORDER BY {} LIMIT 1",
                composer_detail_order("listing").clause()
            ),
            params![artist_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(Some(DbComposerDetail { composer, default_work_id }))
}

fn composer_detail_row_count_on(sql: &SqlReadContext<'_>, artist_id: &str) -> Result<u64, DbError> {
    sql.query_row(
        "SELECT COUNT(*) FROM composer_detail_row WHERE artist_id = ?",
        params![artist_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as u64)
    .map_err(DbError::from)
}

/// The statement reading the rows of one page `seek` reads in `order`, at most
/// `:limit` from `:offset`, each with what it shows and its key: a work with
/// its summary, a credit with its role, album, and for a track credit its
/// track and the composer.
pub(crate) fn composer_detail_rows_query(order: &ListOrder, seek: &Seek) -> String {
    format!(
        "SELECT listing.entry, listing.section, listing.place,
                {opens} AS opens_heading,
                {work},
                rar.id AS release_role_id,
                rar.release_id AS release_role_release_id,
                rar.artist_id AS release_role_artist_id,
                rar.position AS release_role_position,
                rar.source AS release_role_source,
                rar.source_credit AS release_role_source_credit,
                release_credit.name_key AS release_role_credit_key,
                release_credit.name AS release_role_credit_name,
                rar.created_at AS release_role_created_at,
                tar.id AS track_role_id,
                tar.track_id AS track_role_track_id,
                tar.artist_id AS track_role_artist_id,
                tar.position AS track_role_position,
                tar.source AS track_role_source,
                tar.source_credit AS track_role_source_credit,
                track_credit.name_key AS track_role_credit_key,
                track_credit.name AS track_role_credit_name,
                tar.created_at AS track_role_created_at,
                t.id AS track_id, t.release_id AS track_release_id, t.title AS track_title,
                t.medium_id AS track_medium_id, t.side AS track_side,
                t.track_number AS track_track_number, t.duration_ms AS track_duration_ms,
                t.created_at AS track_created_at,
                album.id AS album_id, album.title AS album_title, album.year AS album_year,
                album.is_compilation AS album_is_compilation,
                album.created_at AS album_created_at,
                composer.id AS artist_id, composer.name AS artist_name,
                composer.sort_name AS artist_sort_name,
                composer.discogs_artist_id AS artist_discogs_artist_id,
                composer.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                composer.created_at AS artist_created_at,
                {keys}
         FROM composer_detail_row listing
         LEFT JOIN works w ON listing.section = 0 AND w.id = listing.item_id
         LEFT JOIN release_artist_roles rar ON listing.section = 1 AND rar.id = listing.item_id
         LEFT JOIN track_artist_roles tar ON listing.section = 2 AND tar.id = listing.item_id
         {release_credit}
         {track_credit}
         LEFT JOIN tracks t ON t.id = tar.track_id
         LEFT JOIN releases r ON listing.section <> 0 AND r.id = listing.group_id
         LEFT JOIN albums album ON album.id = r.album_id
         LEFT JOIN artists composer ON listing.section = 2 AND composer.id = listing.artist_id
         WHERE listing.artist_id = :artist_id AND {condition}
         ORDER BY {ordering} LIMIT :limit OFFSET :offset",
        release_credit = credit_name_join("rar", "release_credit"),
        track_credit = credit_name_join("tar", "track_credit"),
        opens = opens_heading_sql(),
        work = work_summary_columns(),
        keys = order.key_columns(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
    )
}

/// The rows of `artist_id`'s page `seek` reads in `order`, at most `limit`
/// from `offset`, each with its key.
pub(crate) fn composer_detail_rows_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbComposerDetailRow>>, DbError> {
    let statement = composer_detail_rows_query(order, seek);
    Ok(seek.bound(limit, offset, |params| {
        let mut params = params.to_vec();
        params.push((":artist_id", &artist_id as &dyn rusqlite::ToSql));
        sql.query(&statement, params.as_slice(), |row| {
            Ok((row_to_composer_detail_row(row)?, order.key_of(row)?))
        })
    })?)
}

/// A row of a composer's page, read by its part of the page: the works
/// (0), the release credits (1) or the track credits (2).
fn row_to_composer_detail_row(row: &Row<'_>) -> rusqlite::Result<DbComposerDetailRow> {
    let section: i64 = row.get("section")?;
    let item = match section {
        0 => DbComposerDetailItem::Work {
            work: row_to_work_summary(row)?,
            part: row.get::<_, i64>("place")? == 1,
        },
        1 => DbComposerDetailItem::ReleaseCredit(DbReleaseRoleSummary {
            role: DbReleaseArtistRole {
                id: row.get("release_role_id")?,
                release_id: row.get("release_role_release_id")?,
                artist_id: row.get("release_role_artist_id")?,
                position: row.get("release_role_position")?,
                source: parsed_column(row, "release_role_source")?,
                source_credit: row.get("release_role_source_credit")?,
                credit: credit_name_column(
                    row,
                    "release_role_credit_key",
                    "release_role_credit_name",
                )?,
                created_at: rfc3339_column(row, "release_role_created_at")?,
            },
            album: row_to_album_with_prefix(row, "album_")?,
        }),
        2 => DbComposerDetailItem::TrackCredit(DbTrackRoleSummary {
            role: DbTrackArtistRole {
                id: row.get("track_role_id")?,
                track_id: row.get("track_role_track_id")?,
                artist_id: row.get("track_role_artist_id")?,
                position: row.get("track_role_position")?,
                source: parsed_column(row, "track_role_source")?,
                source_credit: row.get("track_role_source_credit")?,
                credit: credit_name_column(row, "track_role_credit_key", "track_role_credit_name")?,
                created_at: rfc3339_column(row, "track_role_created_at")?,
            },
            track: row_to_track_with_prefix(row, "track_")?,
            album: row_to_album_with_prefix(row, "album_")?,
            artist: row_to_artist_with_prefix(row, "artist_")?,
        }),
        other => {
            return Err(column_conversion_error(
                row,
                "section",
                format!("composer_detail_row section {other} names no part of the page"),
            ))
        }
    };
    Ok(DbComposerDetailRow {
        entry: row.get("entry")?,
        opens_heading: row.get("opens_heading")?,
        item,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerDetailProjection {
    pub detail: Option<DbComposerDetail>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerDetailRowsProjection {
    pub windows: Vec<bae_store::LibraryBrowseWindow<DbComposerDetailRow>>,
    pub total_count: u64,
}

#[cfg(test)]
#[path = "composer_detail_tests.rs"]
mod tests;
