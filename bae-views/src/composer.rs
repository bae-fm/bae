//! The composer views: the composer list, a composer's detail, and a work's
//! detail.

use super::*;
use crate::album_detail::{
    ComposerDetail, ComposerSummary, ComposerWorkGroup, WorkDetail, WorkReleaseSummary, WorkSummary,
};
use std::collections::HashMap;

impl LibraryViews {
    #[cfg(test)]
    pub(crate) async fn get_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerSummary>, DbError> {
        let order = composer_order(sort);
        self.database
            .read(move |sql| {
                Ok(composer_rows_on(&sql, &order, &order.everything(), limit, offset)?
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect())
            })
            .await
    }

    /// Follow the composer list under `sort` in the windows its request
    /// names, each read from the row beside it when one is known
    /// (`list_windows`).
    pub fn subscribe_composer_browse(
        &self,
        sort: &[ComposerSortCriterion],
        initial_windows: bae_db::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<bae_db::LibraryPageWindows, ComposerBrowseProjection> {
        let order = composer_order(sort);
        let places = KnownPlaces::new();
        self.database.subscribe_reconfigurable(initial_windows, move |requested, sql| {
            let versions = list_order_versions_on(&sql, &["composer_listing"])?;
            let total_count = composer_count_on(&sql).map_err(CovenError::from)?;
            let WindowsRead { windows, found } =
                read_windows(&order, requested, places.under(&versions), |seek, limit, offset| {
                    composer_rows_on(&sql, &order, seek, limit, offset)
                })?;
            places.keep(versions, found);
            Ok(ComposerBrowseProjection {
                windows: windows
                    .into_iter()
                    .map(|(window, rows)| bae_db::LibraryBrowseWindow { window, rows })
                    .collect(),
                total_count,
            })
        })
    }

    #[cfg(test)]
    pub(crate) async fn find_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbComposerDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.database
            .read(move |sql| find_composer_detail_on(&sql, &artist_id))
            .process(|rows| Ok(rows.map(ComposerDetailRows::process)))
            .await
    }

    /// Follow the composer `initial` names, or none. The detail view moves to
    /// another composer through the request handle, not another query.
    pub fn subscribe_composer_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ComposerDetailProjection> {
        self.database
            .subscribe_reconfigurable(initial, |artist_id, sql| {
                let Some(artist_id) = artist_id else {
                    return Ok(None);
                };
                find_composer_detail_on(&sql, artist_id).map_err(CovenError::from)
            })
            .process(|detail| {
                Ok(ComposerDetailProjection { detail: detail.map(ComposerDetailRows::process) })
            })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn find_work_detail(
        &self,
        work_id: &str,
    ) -> Result<Option<DbWorkDetail>, DbError> {
        let work_id = work_id.to_string();
        self.database.read(move |sql| find_work_detail_on(&sql, &work_id)).await
    }

    /// The work's page as its view shows it; `None` for an unknown work.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_work_detail(&self, work_id: &str) -> Result<Option<WorkDetail>, DbError> {
        Ok(self.find_work_detail(work_id).await?.map(work_detail_of))
    }

    /// Follow the work `initial` names, or none. The detail view moves to
    /// another work through the request handle, not another query.
    pub fn subscribe_work_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, WorkDetailProjection> {
        self.database.subscribe_reconfigurable(initial, |work_id, sql| {
            let Some(work_id) = work_id else {
                return Ok(WorkDetailProjection { detail: None });
            };
            Ok(WorkDetailProjection {
                detail: find_work_detail_on(&sql, work_id).map_err(CovenError::from)?,
            })
        })
    }
}

/// One delivery of the composer list as the list shows it.
pub fn resolve_composer_browse(
    projection: ComposerBrowseProjection,
    request_revision: u64,
    cause: coven::LiveQueryCause,
) -> crate::LibraryBrowseSnapshot<ComposerSummary> {
    crate::LibraryBrowseSnapshot {
        windows: projection
            .windows
            .into_iter()
            .map(|window| bae_db::LibraryBrowseWindow {
                window: window.window,
                rows: window.rows.into_iter().map(ComposerSummary::from_raw).collect(),
            })
            .collect(),
        total_count: projection.total_count,
        row_count: projection.total_count,
        sections: Vec::new(),
        request_revision,
        cause,
    }
}

/// One delivery of the composer detail as the detail view shows it: its
/// works grouped under their parents, and the work the view opens first.
pub fn resolve_composer_detail_projection(
    projection: ComposerDetailProjection,
) -> Option<ComposerDetail> {
    let raw = projection.detail?;
    let work_groups = raw
        .work_groups
        .into_iter()
        .map(|group| ComposerWorkGroup {
            id: group.id,
            parent: group.parent.map(WorkSummary::from_raw),
            works: group.works.into_iter().map(WorkSummary::from_raw).collect(),
        })
        .collect::<Vec<_>>();
    let default_work_id = work_groups.first().and_then(|group| {
        group
            .parent
            .as_ref()
            .map(|work| work.raw.work.id.clone())
            .or_else(|| group.works.first().map(|work| work.raw.work.id.clone()))
    });
    Some(ComposerDetail {
        composer: ComposerSummary::from_raw(raw.composer),
        work_groups,
        unlinked_release_roles: raw.unlinked_release_roles,
        unlinked_track_roles: raw.unlinked_track_roles,
        default_work_id,
    })
}

/// One delivery of the work detail as the detail view shows it.
pub fn resolve_work_detail_projection(projection: WorkDetailProjection) -> Option<WorkDetail> {
    projection.detail.map(work_detail_of)
}

/// A work's page as its rows read it, each with its cover's version.
fn work_detail_of(raw: DbWorkDetail) -> WorkDetail {
    WorkDetail {
        work: WorkSummary::from_raw(raw.work),
        child_works: raw.child_works.into_iter().map(WorkSummary::from_raw).collect(),
        releases: raw.releases.into_iter().map(WorkReleaseSummary::from_raw).collect(),
        tracks: raw.tracks,
    }
}

fn find_work_detail_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Option<DbWorkDetail>, DbError> {
    let work = sql
        .query_row(
            &work_summary_query(Some("WHERE w.id = ?"), None),
            params![work_id],
            row_to_work_summary,
        )
        .optional()?;
    let Some(work) = work else {
        return Ok(None);
    };
    let child_works = sql.query(
        &work_summary_query(
            Some("JOIN work_parts wp ON wp.child_work_id = w.id AND wp.parent_work_id = ?"),
            Some("ORDER BY wp.position, w.title"),
        ),
        params![work.work.id],
        row_to_work_summary,
    )?;
    let releases = work_release_rows(sql, &work.work.id)?;
    let tracks = sql.query(
        "SELECT tw.id AS track_work_id, tw.work_id,
                tw.position, tw.source AS link_source, tw.created_at,
                t.id AS track_id, t.release_id AS track_release_id,
                t.title AS track_title, t.medium_id AS track_medium_id, t.side AS track_side,
                t.track_number AS track_track_number,
                t.duration_ms AS track_duration_ms,
                t.created_at AS track_created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?
         ORDER BY a.title, r.album_order, r.id, t.position, tw.position",
        params![work.work.id],
        |row| {
            Ok(DbWorkTrackSummary {
                link: DbTrackWork {
                    id: row.get("track_work_id")?,
                    track_id: row.get("track_id")?,
                    work_id: row.get("work_id")?,
                    position: row.get("position")?,
                    source: parsed_column(row, "link_source")?,
                    created_at: rfc3339_column(row, "created_at")?,
                },
                track: row_to_track_with_prefix(row, "track_")?,
                album: row_to_album_with_prefix(row, "album_")?,
            })
        },
    )?;
    Ok(Some(DbWorkDetail { work, child_works, releases, tracks }))
}

fn find_composer_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<ComposerDetailRows>, DbError> {
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
    let works = sql.query(
        &work_summary_query(
            Some(
                "JOIN composer_work credited ON credited.work_id = w.id AND credited.artist_id = ?",
            ),
            Some("ORDER BY w.title"),
        ),
        params![composer.artist.id],
        row_to_work_summary,
    )?;
    let parent_ids = works
        .iter()
        .filter_map(|work| work.parent_work_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let parent_summaries = if parent_ids.is_empty() {
        HashMap::new()
    } else {
        let placeholders = (0..parent_ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let filter = format!("WHERE w.id IN ({placeholders})");
        let query = work_summary_query(Some(&filter), None);
        sql.query(&query, rusqlite::params_from_iter(parent_ids.iter()), |row| {
            row_to_work_summary(row).map(|summary| (summary.work.id.clone(), summary))
        })?
        .into_iter()
        .collect::<HashMap<_, _>>()
    };
    let release_roles_query = "SELECT rar.id AS release_artist_role_id, rar.release_id,
                rar.artist_id, rar.position,
                rar.source AS role_source, rar.source_credit, rar.created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at
         FROM composer_unlinked_credit unlinked
         JOIN release_artist_roles rar ON rar.id = unlinked.credit_id
         JOIN releases r ON r.id = rar.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE unlinked.artist_id = ?
         ORDER BY a.title, r.album_order, r.id, rar.position";
    let unlinked_release_roles =
        sql.query(release_roles_query, params![composer.artist.id], |row| {
            Ok(DbReleaseRoleSummary {
                role: DbReleaseArtistRole {
                    id: row.get("release_artist_role_id")?,
                    release_id: row.get("release_id")?,
                    artist_id: row.get("artist_id")?,
                    position: row.get("position")?,
                    source: parsed_column(row, "role_source")?,
                    source_credit: row.get("source_credit")?,
                    created_at: rfc3339_column(row, "created_at")?,
                },
                album: row_to_album_with_prefix(row, "album_")?,
            })
        })?;
    let track_roles_query = "SELECT tar.id AS track_artist_role_id, tar.track_id, tar.artist_id,
                tar.position, tar.source AS role_source,
                tar.source_credit, tar.created_at,
                t.id AS track_id, t.release_id AS track_release_id,
                t.title AS track_title, t.medium_id AS track_medium_id, t.side AS track_side,
                t.track_number AS track_track_number,
                t.duration_ms AS track_duration_ms,
                t.created_at AS track_created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at,
                art.id AS artist_id, art.name AS artist_name,
                art.sort_name AS artist_sort_name,
                art.discogs_artist_id AS artist_discogs_artist_id,
                art.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                art.created_at AS artist_created_at
         FROM composer_unlinked_credit unlinked
         JOIN track_artist_roles tar ON tar.id = unlinked.credit_id
         JOIN tracks t ON t.id = tar.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         JOIN artists art ON art.id = unlinked.artist_id
         WHERE unlinked.artist_id = ?
         ORDER BY a.title, r.album_order, r.id, t.position, tar.position";
    let unlinked_track_roles =
        sql.query(track_roles_query, params![composer.artist.id], row_to_track_role_summary)?;
    Ok(Some(ComposerDetailRows {
        composer,
        works,
        parents: parent_summaries,
        unlinked_release_roles,
        unlinked_track_roles,
    }))
}

#[derive(Clone, PartialEq)]
struct ComposerDetailRows {
    composer: DbComposerSummary,
    works: Vec<DbWorkSummary>,
    parents: HashMap<String, DbWorkSummary>,
    unlinked_release_roles: Vec<DbReleaseRoleSummary>,
    unlinked_track_roles: Vec<DbTrackRoleSummary>,
}

impl ComposerDetailRows {
    fn process(self) -> DbComposerDetail {
        let Self {
            composer,
            works,
            parents: parent_summaries,
            unlinked_release_roles,
            unlinked_track_roles,
        } = self;
        let mut grouped: HashMap<String, Vec<DbWorkSummary>> = HashMap::new();
        let mut ungrouped = Vec::new();
        for work in works {
            if let Some(parent_id) = work.parent_work_id.clone() {
                grouped.entry(parent_id).or_default().push(work);
            } else {
                let id = work.work.id.clone();
                ungrouped.push(DbComposerWorkGroup { id, parent: None, works: vec![work] });
            }
        }
        let mut work_groups = grouped
            .into_iter()
            .map(|(parent_id, mut works)| {
                works.sort_by_key(work_summary_sort_key);
                DbComposerWorkGroup {
                    id: parent_id.clone(),
                    parent: parent_summaries.get(&parent_id).cloned(),
                    works,
                }
            })
            .collect::<Vec<_>>();
        work_groups.extend(ungrouped);
        work_groups.sort_by_key(composer_work_group_sort_key);

        DbComposerDetail { composer, work_groups, unlinked_release_roles, unlinked_track_roles }
    }
}

fn composer_work_group_sort_key(group: &DbComposerWorkGroup) -> String {
    if let Some(parent) = group.parent.as_ref() {
        return work_summary_sort_key(parent);
    }
    let work = group.works.first().expect("composer work group without parent must contain a work");
    work_summary_sort_key(work)
}

fn work_summary_sort_key(work: &DbWorkSummary) -> String {
    work.work.title.to_lowercase()
}

fn composer_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM composer_listing", [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(DbError::from)
}

/// The composer list's rows `seek` reads in `order`, at most `limit` from
/// `offset`, each with its key.
pub(crate) fn composer_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbComposerSummary>>, DbError> {
    let statement = listing_window_query(composer_listing_query, order, seek);
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| Ok((row_to_composer_summary(row)?, order.key_of(row)?)))
    })?)
}

/// The composer list's rows (`composer_listing`), each with its artist, as
/// [`row_to_composer_summary`] reads them, and each row's key in `keyed`'s
/// order when one is given; `filter` and `tail` follow the join.
pub(crate) fn composer_listing_query(
    keyed: Option<&ListOrder>,
    filter: Option<&str>,
    tail: Option<&str>,
) -> String {
    let mut query = format!(
        "SELECT composer.id AS artist_id,
                composer.name AS artist_name,
                composer.sort_name AS artist_sort_name,
                composer.discogs_artist_id AS artist_discogs_artist_id,
                composer.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                composer.created_at AS artist_created_at,
                {image} AS artist_image_version,
                listing.work_count,
                listing.linked_release_count,
                listing.unlinked_credit_count{keys}
         FROM composer_listing listing
         JOIN artists composer ON composer.id = listing.artist_id
         ",
        image = artist_image_version_sql("composer.id"),
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

/// The release whose cover a work shows: the release of the first track,
/// in track position, that performs it (a SQL expression over `w`).
const WORK_REPRESENTATIVE_RELEASE: &str = "(
                    SELECT tr.release_id
                    FROM track_works tw_cover
                    JOIN tracks tr ON tr.id = tw_cover.track_id
                    WHERE tw_cover.work_id = w.id
                    ORDER BY tr.position, tr.release_id
                    LIMIT 1
                )";

pub(crate) fn work_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = format!(
        "SELECT w.id AS work_id,
                w.title AS work_title,
                w.disambiguation AS work_disambiguation,
                w.work_type AS work_type,
                w.musicbrainz_work_id AS work_musicbrainz_id,
                w.created_at AS work_created_at,
                (
                    SELECT wp.parent_work_id
                    FROM work_parts wp
                    WHERE wp.child_work_id = w.id
                    ORDER BY wp.position, wp.parent_work_id
                    LIMIT 1
                ) AS parent_work_id,
                {WORK_REPRESENTATIVE_RELEASE} AS representative_release_id,
                {representative_cover} AS representative_cover_version,
                (
                    SELECT GROUP_CONCAT(composer.name, ', ' ORDER BY credit.position)
                    FROM (
                        SELECT {work_artist} AS artist_id, MIN(wa.position) AS position
                        FROM work_artists wa
                        WHERE wa.work_id = w.id
                        GROUP BY 1
                    ) credit
                    JOIN artists composer ON composer.id = credit.artist_id
                ) AS composer_names,
                COUNT(DISTINCT tr.release_id) AS linked_release_count
         FROM works w
         LEFT JOIN track_works tw ON tw.work_id = w.id
         LEFT JOIN tracks tr ON tr.id = tw.track_id
         ",
        work_artist = shown_artist_id("wa.artist_id"),
        representative_cover = cover_version_sql(WORK_REPRESENTATIVE_RELEASE),
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    query.push_str(
        "
         GROUP BY w.id, w.title, w.disambiguation, w.work_type, w.musicbrainz_work_id, \
                  w.created_at",
    );
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

pub(crate) fn row_to_work_summary(row: &Row<'_>) -> rusqlite::Result<DbWorkSummary> {
    let representative_release_id: Option<String> = row.get("representative_release_id")?;
    let representative_cover = match &representative_release_id {
        Some(id) => bae_db::cover_ref(id, row.get("representative_cover_version")?),
        None => None,
    };
    Ok(DbWorkSummary {
        work: DbWork {
            id: row.get("work_id")?,
            title: row.get("work_title")?,
            disambiguation: row.get("work_disambiguation")?,
            work_type: row.get("work_type")?,
            musicbrainz_work_id: row.get("work_musicbrainz_id")?,
            created_at: rfc3339_column(row, "work_created_at")?,
        },
        parent_work_id: row.get("parent_work_id")?,
        representative_release_id,
        representative_cover,
        composer_names: row.get("composer_names")?,
        linked_release_count: row.get("linked_release_count")?,
    })
}

pub(crate) fn row_to_composer_summary(row: &Row<'_>) -> rusqlite::Result<DbComposerSummary> {
    let artist = row_to_artist_with_prefix(row, "artist_")?;
    Ok(DbComposerSummary {
        image: bae_db::artist_image_ref(&artist.id, row.get("artist_image_version")?),
        artist,
        work_count: row.get("work_count")?,
        linked_release_count: row.get("linked_release_count")?,
        unlinked_credit_count: row.get("unlinked_credit_count")?,
    })
}

fn row_to_track_role_summary(row: &Row<'_>) -> rusqlite::Result<DbTrackRoleSummary> {
    Ok(DbTrackRoleSummary {
        role: DbTrackArtistRole {
            id: row.get("track_artist_role_id")?,
            track_id: row.get("track_id")?,
            artist_id: row.get("artist_id")?,
            position: row.get("position")?,
            source: parsed_column(row, "role_source")?,
            source_credit: row.get("source_credit")?,
            created_at: rfc3339_column(row, "created_at")?,
        },
        track: row_to_track_with_prefix(row, "track_")?,
        album: row_to_album_with_prefix(row, "album_")?,
        artist: row_to_artist_with_prefix(row, "artist_")?,
    })
}

fn work_release_rows(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Vec<DbWorkReleaseSummary>, DbError> {
    sql.query(
        &format!(
            "SELECT DISTINCT
            r.id AS release_id,
            r.album_id,
            a.title AS album_title,
            r.release_name,
            r.year AS release_year,
            {media} AS release_media,
            (
                SELECT COUNT(*)
                FROM releases indexed_release
                WHERE indexed_release.album_id = r.album_id
                  AND (
                      indexed_release.album_order < r.album_order
                      OR (
                          indexed_release.album_order = r.album_order
                          AND indexed_release.id <= r.id
                      )
                  )
            ) AS release_index,
            {cover} AS cover_version
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?
         ORDER BY a.title, r.album_order, r.id",
            cover = cover_version_sql("r.id"),
            media = release_media_sql("r.id"),
        ),
        params![work_id],
        row_to_work_release_summary,
    )
    .map_err(DbError::from)
}

fn row_to_work_release_summary(row: &Row<'_>) -> rusqlite::Result<DbWorkReleaseSummary> {
    Ok(DbWorkReleaseSummary {
        release_id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        release_name: row.get("release_name")?,
        year: row.get("release_year")?,
        media: read_media(row, "release_media")?,
        release_index: row.get("release_index")?,
        cover_version: row.get("cover_version")?,
    })
}

/// The composer list's order, over `composer_listing listing`. Ties break by
/// name ascending, then id — a window needs a total order and a name is not
/// unique — and that tie-break is the whole order when `sort` is empty. A
/// name the sort already orders by is not repeated, so an index of
/// `composer_listing` serves each one-field sort. Mirrors `album_order`'s
/// `album_id` tie-break.
pub(crate) fn composer_order(sort: &[ComposerSortCriterion]) -> ListOrder {
    let mut terms = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ComposerSortField::Name => "listing.name",
                ComposerSortField::WorkCount => "listing.work_count",
                ComposerSortField::LinkedReleaseCount => "listing.linked_release_count",
            };
            SortTerm::running(field, c.direction)
        })
        .collect::<Vec<_>>();
    if !sort.iter().any(|c| c.field == ComposerSortField::Name) {
        terms.push(SortTerm::ascending("listing.name"));
    }
    terms.push(SortTerm::ascending("listing.artist_id"));
    ListOrder::new(terms)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerBrowseProjection {
    pub windows: Vec<bae_db::LibraryBrowseWindow<DbComposerSummary>>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerDetailProjection {
    pub detail: Option<DbComposerDetail>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkDetailProjection {
    pub detail: Option<DbWorkDetail>,
}
