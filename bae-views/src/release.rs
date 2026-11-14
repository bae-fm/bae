//! The release detail view: one release with its album's credits, its place
//! among the album's releases, and the state it is resolved against.

use super::*;
use crate::album_detail::{ReleaseDetail, ReleaseResolveCtx};

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
        release_index,
        is_compilation,
        album_year,
    } = context;
    let ctx = ReleaseResolveCtx {
        cover: bae_records::cover_ref(release_id, detail.cover_version.clone()),
        is_compilation,
        album_year,
    };
    ReleaseDetail::from_raw(detail, download, actions, release_index, &ctx)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
}

/// How many releases of its album come before the release `?1` in the
/// album's order (`album_order`, then id): its index among them, counted on
/// the album's releases index.
pub(crate) const RELEASE_INDEX_IN_ALBUM: &str = "SELECT COUNT(*) FROM releases AS shown \
     JOIN releases AS earlier ON earlier.album_id = shown.album_id \
      AND (earlier.album_order < shown.album_order \
           OR (earlier.album_order = shown.album_order AND earlier.id < shown.id)) \
     WHERE shown.id = ?1";

/// The release's detail rows, its index among the album's releases, and whether the album is a compilation; `None` when
/// the release doesn't exist.
pub(crate) fn find_release_detail_context_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseContextRows>, DbError> {
    let Some(detail) = build_release_detail_on(sql, release_id)? else {
        return Ok(None);
    };
    let release_index: i64 =
        sql.query_row(RELEASE_INDEX_IN_ALBUM, params![release_id], |row| row.get(0))?;
    let release_index =
        bae_store::query::to_usize(release_index, "a release's index in its album")?;
    let album = find_album_by_id_on(sql, &detail.rows.release().album_id)?;
    let is_compilation = album.as_ref().is_some_and(|album| album.is_compilation);
    let album_year = album.and_then(|album| album.year);
    Ok(Some(ReleaseContextRows { detail, release_index, is_compilation, album_year }))
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseContextRows {
    detail: ReleaseDetailRows,
    release_index: usize,
    is_compilation: bool,
    album_year: Option<i32>,
}

impl ReleaseContextRows {
    pub(crate) fn process(self) -> ReleaseDetailContext {
        let ReleaseDetailRead { detail, download, actions } = self.detail.process();
        ReleaseDetailContext {
            detail,
            download,
            actions,
            release_index: self.release_index,
            is_compilation: self.is_compilation,
            album_year: self.album_year,
        }
    }
}

/// The columns beside a release `rel`'s detail rows: its entry in the
/// download queue and the storage actions it offers.
fn release_standing_columns() -> String {
    format!(
        "{}, {}",
        bae_transfers::download_entry_sql("rel.id"),
        crate::storage_actions::storage_action_columns("rel.id")
    )
}

type ReleaseStanding = (Option<bae_transfers::DownloadEntry>, crate::DbStorageActions);

/// The standing of a release as [`release_standing_columns`] read it.
fn release_standing(row: &Row<'_>) -> rusqlite::Result<ReleaseStanding> {
    Ok((bae_transfers::download_entry_column(row)?, crate::DbStorageActions::of_row(row)?))
}

/// The release's rows as its detail reads them, beside its entry in the
/// download queue and the storage actions it offers; `None` when the release
/// doesn't exist.
pub(crate) fn build_release_detail_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseDetailRows>, DbError> {
    Ok(release_detail_rows_on(
        sql,
        release_id,
        &bae_people::people::own_track_stars_sql("track"),
        &release_standing_columns(),
        release_standing,
    )?
    .map(|(rows, (download, actions))| ReleaseDetailRows { rows, download, actions }))
}

/// The detail rows of each release of the album `album_id`, in the album's
/// order, each beside its entry in the download queue and its storage
/// actions, in one statement.
pub(crate) fn build_album_release_details_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Vec<ReleaseDetailRows>, DbError> {
    Ok(bae_records::records::release_details_on(
        sql,
        bae_records::records::ReleaseSet::OfAlbum(album_id),
        &bae_people::people::own_track_stars_sql("track"),
        &release_standing_columns(),
        release_standing,
    )?
    .into_iter()
    .map(|(rows, (download, actions))| ReleaseDetailRows { rows, download, actions })
    .collect())
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseDetailRows {
    rows: DbReleaseDetailRows,
    download: Option<bae_transfers::DownloadEntry>,
    actions: crate::DbStorageActions,
}

impl ReleaseDetailRows {
    pub(crate) fn process(self) -> ReleaseDetailRead {
        ReleaseDetailRead {
            detail: self.rows.process(),
            download: self.download,
            actions: self.actions,
        }
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
