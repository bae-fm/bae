//! The release detail view: one release with its album's credits, its place
//! among the album's releases, and the state it is resolved against.

use super::*;
use crate::album_detail::{ReleaseDetail, ReleaseResolveCtx};
use bae_people::people::{RatingSystem, RATING_SYSTEMS_SQL};

impl LibraryViews {
    /// Resolved release detail for the album-detail view: a `ReleaseSummary` plus
    /// the tracks/files/gallery its SQL joins load, and the release's position
    /// within its album, so `display_name` needs no index from the caller. `None`
    /// when the release doesn't exist.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetail>, DbError> {
        let Some(context) = self.find_release_detail_context(release_id).await? else {
            return Ok(None);
        };
        Ok(Some(resolve_release_detail(release_id, context)))
    }

    /// The release's rows as its detail reads them, unresolved; `None` when
    /// the release doesn't exist.
    pub async fn find_db_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseDetail>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| build_release_detail_on(&sql, &release_id))
            .process(|rows| Ok(rows.map(|rows| rows.process().detail)))
            .await
    }

    /// The raw release-detail aggregate and its 0-based position among its album's releases — all from one database call.
    pub(crate) async fn find_release_detail_context(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetailContext>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| find_release_detail_context_on(&sql, &release_id))
            .process(|rows| Ok(rows.map(ReleaseContextRows::process)))
            .await
    }

    /// Follow the release `initial` names, or none. The detail view moves
    /// to another release through the request handle, not another query.
    pub fn subscribe_release_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ReleaseDetailProjection, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |release_id, sql| match release_id {
                Some(release_id) => find_release_detail_context_on(&sql, release_id),
                None => Ok(None),
            })
            .process(|context| {
                Ok(ReleaseDetailProjection { context: context.map(ReleaseContextRows::process) })
            })
    }
}

/// One delivery of the release `release_id` as the detail view shows it.
pub fn resolve_release_detail_projection(
    release_id: &str,
    projection: ReleaseDetailProjection,
) -> Option<ReleaseDetail> {
    projection.context.map(|context| resolve_release_detail(release_id, context))
}

/// The release's detail as its view shows it.
fn resolve_release_detail(release_id: &str, context: ReleaseDetailContext) -> ReleaseDetail {
    let ReleaseDetailContext {
        detail,
        download,
        actions,
        is_compilation,
        album_year,
        tracks_rating_system,
    } = context;
    let ctx = ReleaseResolveCtx {
        cover: bae_records::cover_ref(release_id, detail.cover_version.clone()),
        is_compilation,
        album_year,
    };
    ReleaseDetail::from_raw(detail, download, actions, &ctx, tracks_rating_system)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
}

/// The release's detail rows beside its album's compilation flag and year, in
/// one statement; `None` when the release doesn't exist.
pub(crate) fn find_release_detail_context_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseContextRows>, DbError> {
    Ok(release_detail_rows_on(
        sql,
        release_id,
        &bae_people::people::own_track_stars_sql("track"),
        &format!(
            "{}, \
             (SELECT a.is_compilation FROM albums a WHERE a.id = rel.album_id) \
                 AS album_is_compilation, \
             (SELECT a.year FROM albums a WHERE a.id = rel.album_id) AS album_year",
            release_detail_columns()
        ),
        release_context_columns,
    )?
    .map(|(rows, (columns, is_compilation, album_year))| ReleaseContextRows {
        detail: ReleaseDetailRows { rows, columns },
        is_compilation,
        album_year,
    }))
}

/// The columns of [`find_release_detail_context_on`]'s statement beside the
/// release's rows and its album's compilation flag and year.
fn release_context_columns(
    row: &Row<'_>,
) -> rusqlite::Result<(ReleaseDetailColumns, bool, Option<i32>)> {
    Ok((ReleaseDetailColumns::read(row)?, row.get("album_is_compilation")?, row.get("album_year")?))
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseContextRows {
    detail: ReleaseDetailRows,
    is_compilation: bool,
    album_year: Option<i32>,
}

impl ReleaseContextRows {
    pub(crate) fn process(self) -> ReleaseDetailContext {
        let ReleaseDetailRead { detail, download, actions, tracks_rating_system } =
            self.detail.process();
        ReleaseDetailContext {
            detail,
            download,
            actions,
            is_compilation: self.is_compilation,
            album_year: self.album_year,
            tracks_rating_system,
        }
    }
}

/// The columns beside a release `rel`'s detail rows: its entry in the
/// download queue, its storage actions and how its track ratings show.
fn release_detail_columns() -> String {
    format!(
        "{}, {}, (SELECT tracks FROM ({RATING_SYSTEMS_SQL})) AS tracks_rating_system",
        bae_transfers::download_entry_sql("rel.id"),
        crate::storage_actions::storage_action_columns("rel.id")
    )
}

#[derive(Clone, PartialEq)]
struct ReleaseDetailColumns {
    download: Option<bae_transfers::DownloadEntry>,
    actions: crate::DbStorageActions,
    tracks_rating_system: RatingSystem,
}

impl ReleaseDetailColumns {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            download: bae_transfers::download_entry_column(row)?,
            actions: crate::DbStorageActions::of_row(row)?,
            tracks_rating_system: bae_store::read::parsed_column(row, "tracks_rating_system")?,
        })
    }
}

/// The release's rows as its detail reads them, beside its entry in the
/// download queue and the storage actions it offers; `None` when the release
/// doesn't exist.
pub(crate) fn build_release_detail_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseDetailRows>, DbError> {
    Ok(find_release_detail_context_on(sql, release_id)?.map(|context| context.detail))
}

/// The detail rows of each release of the album `album_id`, in the album's
/// order, with each release's download entry, storage actions and track
/// rating system, in one statement.
pub(crate) fn build_album_release_details_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Vec<ReleaseDetailRows>, DbError> {
    Ok(bae_records::records::release_details_on(
        sql,
        bae_records::records::ReleaseSet::OfAlbum(album_id),
        &bae_people::people::own_track_stars_sql("track"),
        &release_detail_columns(),
        ReleaseDetailColumns::read,
    )?
    .into_iter()
    .map(|(rows, columns)| ReleaseDetailRows { rows, columns })
    .collect())
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseDetailRows {
    rows: DbReleaseDetailRows,
    columns: ReleaseDetailColumns,
}

impl ReleaseDetailRows {
    pub(crate) fn process(self) -> ReleaseDetailRead {
        ReleaseDetailRead {
            detail: self.rows.process(),
            download: self.columns.download,
            actions: self.columns.actions,
            tracks_rating_system: self.columns.tracks_rating_system,
        }
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
