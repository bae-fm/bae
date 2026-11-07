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
        let syncs = self.store.syncs().await?;
        Ok(Some(resolve_release_detail(release_id, context, syncs)))
    }

    /// The release's rows as its detail reads them, unresolved; `None` when
    /// the release doesn't exist.
    pub async fn find_db_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseDetail>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                let Some(release) = find_release_by_id_on(&sql, &release_id)? else {
                    return Ok(None);
                };
                Ok(Some(build_release_detail_on(&sql, release)?))
            })
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
            .subscribe_reconfigurable(initial, |release_id, sql| {
                let syncs = syncs_on(&sql)?;
                let context = match release_id {
                    Some(release_id) => find_release_detail_context_on(&sql, release_id)?,
                    None => None,
                };
                Ok((context, syncs))
            })
            .process(|(context, syncs)| {
                Ok(ReleaseDetailProjection {
                    context: context.map(ReleaseContextRows::process),
                    syncs,
                })
            })
    }
}

/// One delivery of the release `release_id` as the detail view shows it.
pub fn resolve_release_detail_projection(
    release_id: &str,
    projection: ReleaseDetailProjection,
) -> Option<ReleaseDetail> {
    let syncs = projection.syncs;
    projection.context.map(|context| resolve_release_detail(release_id, context, syncs))
}

/// The release's detail as its view shows it, against whether storage is
/// set up.
fn resolve_release_detail(
    release_id: &str,
    context: ReleaseDetailContext,
    syncs: bool,
) -> ReleaseDetail {
    let ReleaseDetailContext { detail, transfers, release_index, is_compilation } = context;
    let ctx = ReleaseResolveCtx {
        syncs,
        cover: bae_records::cover_ref(release_id, detail.cover_version.clone()),
        is_compilation,
    };
    ReleaseDetail::from_raw(detail, transfers, release_index, &ctx)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
    /// Whether storage is set up on this device, as coven's sync status
    /// says.
    pub syncs: bool,
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
    let Some(release) = find_release_by_id_on(sql, release_id)? else {
        return Ok(None);
    };
    let release_index: i64 =
        sql.query_row(RELEASE_INDEX_IN_ALBUM, params![release.id], |row| row.get(0))?;
    let release_index =
        bae_store::query::to_usize(release_index, "a release's index in its album")?;
    let is_compilation =
        find_album_by_id_on(sql, &release.album_id)?.is_some_and(|album| album.is_compilation);
    Ok(Some(ReleaseContextRows {
        detail: build_release_detail_on(sql, release)?,
        release_index,
        is_compilation,
    }))
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseContextRows {
    detail: ReleaseDetailRows,
    release_index: usize,
    is_compilation: bool,
}

impl ReleaseContextRows {
    pub(crate) fn process(self) -> ReleaseDetailContext {
        let ReleaseDetailRead { detail, transfers } = self.detail.process();
        ReleaseDetailContext {
            detail,
            transfers,
            release_index: self.release_index,
            is_compilation: self.is_compilation,
        }
    }
}

/// The release's rows as its detail reads them, beside what the transfer
/// queues and the storage transitions say of it.
pub(crate) fn build_release_detail_on(
    sql: &SqlReadContext<'_>,
    release: DbRelease,
) -> Result<ReleaseDetailRows, DbError> {
    let transfers = bae_transfers::release_transfer_state_on(sql, &release.id)?;
    Ok(ReleaseDetailRows { rows: release_detail_rows_on(sql, release)?, transfers })
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseDetailRows {
    rows: DbReleaseDetailRows,
    transfers: bae_transfers::ReleaseTransferState,
}

impl ReleaseDetailRows {
    pub(crate) fn process(self) -> ReleaseDetailRead {
        ReleaseDetailRead { detail: self.rows.process(), transfers: self.transfers }
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
