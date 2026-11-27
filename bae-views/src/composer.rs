//! The composer views: the composer list, a work's detail and a work's
//! children as a list. A composer's own page is `composer_detail`.
//!
//! Its tables and the triggers keeping them are `composer.sql`.

use super::*;
use crate::album_detail::{WorkChild, WorkDetail, WorkReleaseSummary};
use bae_records::records::browse_fragments::{WorkReleaseRow, WorkSummary};
use bae_records::records::files::ArtistImageVersion;
use bae_records::records::files::CoverVersion;
use bae_store::{statement_optional, statement_row, statement_rows};

mod list;
mod window_reads;
pub(crate) use list::{
    composer_key_at_on, composer_list_length_on, composer_page_on, composer_sort,
    composer_window_on, composers_before_on,
};

impl LibraryViews {
    /// The composer list's rows at the places from `offset`, at most `limit`.
    #[cfg(test)]
    pub(crate) async fn get_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerSummary>, DbError> {
        let sort = composer_sort(sort);
        let page = ListPage { offset, limit };
        self.store.read(move |sql| composer_page_on(&sql, &sort, page)).await
    }

    /// Follow one window of the composer list under `sort`: the rows from a key
    /// of its order on.
    pub fn subscribe_composer_window(
        &self,
        sort: &[ComposerSortCriterion],
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbComposerSummary>, bae_store::DbError>
    {
        let sort = composer_sort(sort);
        self.store.subscribe_reconfigurable(initial, move |window, sql| {
            composer_window_on(&sql, &sort, window)
        })
    }

    /// Follow one page of the composer list under `sort`: the composers at the
    /// page's places as each write leaves them.
    pub fn subscribe_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let sort = composer_sort(sort);
        self.store
            .subscribe(move |sql| composer_page_on(&sql, &sort, page))
            .process(|rows| Ok(crate::ListRows::Composers(rows)))
    }

    /// Follow how many composers the list has. The count runs as a separate
    /// query from the windows, because every composer added or removed changes
    /// it.
    pub fn subscribe_composer_count(&self) -> coven::LiveQuery<u64, bae_store::DbError> {
        self.store.subscribe(|sql| composer_list_length_on(&sql))
    }

    /// Follow how many composers come before the row at a key; the caller
    /// changes the key on the same query.
    pub fn subscribe_composer_position(
        &self,
        sort: &[ComposerSortCriterion],
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let sort = composer_sort(sort);
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            composers_before_on(&sql, &sort, key)
        })
    }

    /// The key of the composer at `position`, or of the last one when the list
    /// ends before it; `None` for an empty list.
    pub async fn composer_key_at(
        &self,
        sort: &[ComposerSortCriterion],
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let sort = composer_sort(sort);
        self.store.read(move |sql| composer_key_at_on(&sql, &sort, position)).await
    }

    /// The composer `artist_id` as a media browser's item shows it, read
    /// once; `None` when the composer list doesn't show it.
    pub async fn find_composer_item(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbComposerSummary>, DbError> {
        let statement = ListedComposerByIdParams { artist: artist_id.to_string() };
        self.store
            .read(move |sql| {
                statement_optional(&sql, &statement)?
                    .map(|row| row.shown.into_summary())
                    .transpose()
            })
            .await
    }

    /// The work `work_id` as a media browser's item shows it, read once;
    /// `None` for an unknown work.
    pub async fn find_work_item(&self, work_id: &str) -> Result<Option<DbWorkSummary>, DbError> {
        let statement = WorkByIdParams { work: work_id.to_string() };
        self.store
            .read(move |sql| {
                statement_optional(&sql, &statement)?
                    .map(|row| work_summary(row.work, row.cover))
                    .transpose()
            })
            .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn find_work_detail(
        &self,
        work_id: &str,
    ) -> Result<Option<DbWorkDetail>, DbError> {
        let work_id = work_id.to_string();
        self.store.read(move |sql| find_work_detail_on(&sql, &work_id)).await
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
    ) -> coven::ReconfigurableLiveQuery<Option<String>, WorkDetailProjection, bae_store::DbError>
    {
        self.store.subscribe_reconfigurable(initial, |work_id, sql| {
            let Some(work_id) = work_id else {
                return Ok(WorkDetailProjection { detail: None });
            };
            Ok(WorkDetailProjection { detail: find_work_detail_on(&sql, work_id)? })
        })
    }

    /// Follow one page of the work `work_id`'s children: the children at the
    /// page's places as each write leaves them, as a media browser lists
    /// them.
    pub fn subscribe_work_child_page(
        &self,
        work_id: &str,
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let work_id = work_id.to_string();
        self.store
            .subscribe(move |sql| work_child_page_on(&sql, &work_id, page))
            .process(|rows| Ok(work_child_rows(rows)))
    }

    /// Follow how many children the work `work_id` has. The count runs as a
    /// separate query from the pages' rows.
    pub fn subscribe_work_child_count(
        &self,
        work_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let work_id = work_id.to_string();
        self.store.subscribe(move |sql| {
            let statement = WorkChildCountParams { work: work_id.clone() };
            list_count(statement_row(&sql, &statement)?.children)
        })
    }
}

/// One delivery of a composer list window as the list shows it.
pub fn resolve_composer_window(read: WindowRead<DbComposerSummary>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, crate::ListRows::Composers)
}

/// A work's children as a media browser lists them.
fn work_child_rows(rows: Vec<DbWorkChild>) -> crate::ListRows {
    crate::ListRows::WorkChildren(
        rows.into_iter()
            .map(|child| match child {
                DbWorkChild::Part(work) => WorkChild::Part(work),
                DbWorkChild::Release(release) => {
                    WorkChild::Release(WorkReleaseSummary::from_raw(release))
                }
            })
            .collect(),
    )
}

/// One delivery of the work detail as the detail view shows it.
pub fn resolve_work_detail_projection(projection: WorkDetailProjection) -> Option<WorkDetail> {
    projection.detail.map(work_detail_of)
}

/// A work's page as its rows read it, each with its cover's version.
fn work_detail_of(raw: DbWorkDetail) -> WorkDetail {
    WorkDetail {
        work: raw.work,
        child_works: raw.child_works,
        releases: raw.releases.into_iter().map(WorkReleaseSummary::from_raw).collect(),
        tracks: raw.tracks,
        child_work_count: raw.child_work_count,
        release_count: raw.release_count,
        track_count: raw.track_count,
    }
}

fn find_work_detail_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Option<DbWorkDetail>, DbError> {
    let work = || work_id.to_string();
    let Some(found) = statement_optional(sql, &WorkByIdParams { work: work() })? else {
        return Ok(None);
    };
    let parts = statement_rows(sql, &WorkPartRowsParams { work: work() })?;
    // Each section's statement counts its own rows beside them; a section
    // with no rows counts zero.
    let child_work_count = list_count(parts.first().map_or(0, |part| part.children))?;
    let child_works =
        parts.into_iter().map(|row| work_summary(row.part, row.cover)).collect::<Result<_, _>>()?;
    let performing = statement_rows(sql, &WorkReleaseRowsParams { work: work() })?;
    let release_count = list_count(performing.first().map_or(0, |release| release.children))?;
    let releases = performing
        .into_iter()
        .map(|row| work_release(row.release, row.cover))
        .collect::<Result<_, _>>()?;
    let performed = statement_rows(sql, &WorkTrackRowsParams { work: work() })?;
    let track_count = list_count(performed.first().map_or(0, |track| track.listed))?;
    let tracks = performed
        .into_iter()
        .map(|row| {
            Ok(DbWorkTrackSummary {
                link: row.link.into_link()?,
                track: row.track.into_track()?,
                album: row.album.into_album()?,
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(Some(DbWorkDetail {
        work: work_summary(found.work, found.cover)?,
        child_works,
        releases,
        tracks,
        child_work_count,
        release_count,
        track_count,
    }))
}

// Each artist the composer list shows (`composer_listing`), with how many
// works it is credited on and how many releases perform them, and its image's
// version.
sqlive_compose::fragment!(
    ListedComposer,
    r#"
    SELECT listing.artist_id AS listed_id, listing.work_count, listing.linked_release_count, artist.*, image.*
    FROM composer_listing AS listing
    JOIN @bae_records::records::browse_fragments::ArtistRecord AS artist
      ON artist.artist_id = listing.artist_id
    LEFT JOIN @bae_records::records::files::ArtistImageVersion AS image
           ON image.id = listing.artist_id
"#
);

impl ListedComposer {
    /// The composer as its row shows it.
    pub(crate) fn into_summary(self) -> Result<DbComposerSummary, DbError> {
        let artist = self.artist.into_artist()?;
        let version = self.image.and_then(ArtistImageVersion::into_version);
        Ok(DbComposerSummary {
            image: bae_records::artist_image_ref(&artist.id, version),
            artist,
            work_count: self.work_count,
            linked_release_count: self.linked_release_count,
        })
    }
}

// The composer `artist`, as the composer list shows it; no row for an artist
// the list doesn't show.
sqlive_compose::read!(
    ListedComposerById(artist: String),
    r#"
    SELECT shown.*
    FROM @crate::composer::ListedComposer AS shown
    WHERE shown.listed_id = :artist
"#
);

// The first `limit` composers library search finds for `pattern`, in name order,
// ties going by id, each beside how many rows the statement lists.
sqlive_compose::read!(
    writer FirstComposersFound(pattern: String, limit: i64) { listed: i64 },
    r#"
    SELECT shown.*, COUNT(*) OVER () AS listed
    FROM (SELECT listing.artist_id, listing.name_sort_key
          FROM composer_listing AS listing
          WHERE listing.artist_id IN (
              SELECT matched.artist_id
              FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS matched)
          ORDER BY listing.name_sort_key, listing.artist_id
          LIMIT :limit) AS found
    CROSS JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = found.artist_id
    ORDER BY found.name_sort_key, found.artist_id
"#
);

/// The composers [`FirstComposersFound`] lists for `pattern`, at most `limit`,
/// with how many it lists.
pub(crate) fn first_composers_found_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<(Vec<DbComposerSummary>, u64), DbError> {
    let statement = FirstComposersFoundParams { pattern: pattern.to_string(), limit };
    let rows = statement_rows(sql, &statement)?;
    let listed = list_count(rows.first().map_or(0, |row| row.listed))?;
    let found = rows.into_iter().map(|row| row.shown.into_summary()).collect::<Result<_, _>>()?;
    Ok((found, listed))
}

// The work `work`, as its rows show it, with its cover.
sqlive_compose::read!(
    writer WorkById(work: String),
    r#"
    SELECT work.*, cover.*
    FROM @bae_records::records::browse_fragments::WorkSummary AS work
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = work.representative_release_id
    WHERE work.work_id = :work
"#
);

// The works that are parts of the work `work`, in its page's order, each
// beside how many there are.
sqlive_compose::read!(
    writer WorkPartRows(work: String) { children: i64 },
    r#"
    SELECT part.*, cover.*, COUNT(*) OVER () AS children
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
    JOIN @bae_records::records::browse_fragments::WorkSummary AS part
      ON part.work_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = part.representative_release_id
    WHERE child.section = 0
    ORDER BY child.place, child.title_key, child.item_id
"#
);

// The releases that perform the work `work`, in its page's order, each beside
// how many there are.
sqlive_compose::read!(
    writer WorkReleaseRows(work: String) { children: i64 },
    r#"
    SELECT release.*, cover.*, COUNT(*) OVER () AS children
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
    JOIN @bae_records::records::browse_fragments::WorkReleaseRow AS release
      ON release.release_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = release.release_id
    WHERE child.section = 1
    ORDER BY child.title_key, child.album_id, child.album_order, child.item_id
"#
);

// The tracks that perform the work `work`, each beside how many there are:
// release by release in its page's order, each release's tracks in playing
// order.
sqlive_compose::read!(
    writer WorkTrackRows(work: String) { listed: i64 },
    r#"
    SELECT link.*, track.*, album.*, COUNT(*) OVER () AS listed
    FROM @bae_records::records::browse_fragments::WorkPerformances AS shown
    JOIN @bae_records::records::browse_fragments::TrackWorkLink AS link
      ON link.link_id = shown.link_id
    JOIN @bae_records::records::browse_fragments::TrackRecord AS track
      ON track.track_id = link.track_id
    JOIN @bae_records::records::browse_fragments::AlbumRecord AS album
      ON album.album_id = shown.album_id
    WHERE shown.work_id = :work
    ORDER BY shown.title_key, shown.album_id, shown.album_order, shown.release_id,
             link.track_position, link.position
"#
);

// The children of the work `work` at the places from `offset`, at most
// `limit`, in their order: its parts, then the releases that perform it, each
// with what it shows.
sqlive_compose::read!(
    writer WorkChildRows(work: String, offset: i64, limit: i64),
    r#"
    SELECT child.*, part.*, part_cover.*, release.*, release_cover.*
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
    LEFT JOIN @bae_records::records::browse_fragments::WorkSummary AS part
           ON child.section = 0 AND part.work_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS part_cover
           ON part_cover.id = part.representative_release_id
    LEFT JOIN @bae_records::records::browse_fragments::WorkReleaseRow AS release
           ON child.section = 1 AND release.release_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS release_cover
           ON release_cover.id = release.release_id
    ORDER BY child.section, child.place, child.title_key, child.album_id, child.album_order,
             child.item_id
    LIMIT :limit OFFSET :offset
"#
);

// How many children the work `work` has.
sqlive_compose::read!(
    writer WorkChildCount(work: String) { children: i64 },
    r#"
    SELECT COUNT(*) AS children
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
"#
);

/// A work as its rows show it, with the cover [`CoverVersion`] read for the
/// release whose cover it shows.
pub(crate) fn work_summary(
    work: WorkSummary,
    cover: Option<CoverVersion>,
) -> Result<DbWorkSummary, DbError> {
    let summarized = work.into_summarized()?;
    let representative_cover = match &summarized.representative_release_id {
        Some(id) => bae_records::cover_ref(id, cover.and_then(CoverVersion::into_version)),
        None => None,
    };
    Ok(DbWorkSummary {
        work: summarized.work,
        representative_cover,
        composer_names: summarized.composer_names,
        linked_release_count: summarized.linked_release_count,
    })
}

/// A release that performs a work, with its cover.
fn work_release(
    release: WorkReleaseRow,
    cover: Option<CoverVersion>,
) -> Result<DbWorkReleaseSummary, DbError> {
    let release = release.into_release()?;
    Ok(DbWorkReleaseSummary {
        release_id: release.release_id,
        album_id: release.album_id,
        album_title: release.album_title,
        release_name: release.release_name,
        year: release.year,
        media: release.media,
        release_number: release.release_number,
        cover_version: cover.and_then(CoverVersion::into_version),
    })
}

/// The children of the work `work_id` at `page`'s places.
fn work_child_page_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
    page: ListPage,
) -> Result<Vec<DbWorkChild>, DbError> {
    let statement = WorkChildRowsParams {
        work: work_id.to_string(),
        offset: bae_store::list_windows::sql_bound(page.offset),
        limit: bae_store::list_windows::sql_bound(page.limit),
    };
    statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| {
            let missing =
                || DbError::unreadable("a work child with nothing to show", row.child.clone());
            Ok(if row.child.is_part()? {
                let part = row.part.ok_or_else(missing)?;
                DbWorkChild::Part(work_summary(part, row.part_cover)?)
            } else {
                let release = row.release.ok_or_else(missing)?;
                DbWorkChild::Release(work_release(release, row.release_cover)?)
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkDetailProjection {
    pub detail: Option<DbWorkDetail>,
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "composer_work_children_tests.rs"]
mod work_children_tests;
