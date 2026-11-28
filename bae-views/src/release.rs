//! The release detail view: one release with its album's credits, its place
//! among the album's releases, and the state it is resolved against.

use super::*;
use crate::album_detail::{ReleaseDetail, ReleaseResolveCtx};
use bae_store::statement_optional;

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
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_db_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseDetail>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| find_release_detail_context_on(&sql, &release_id))
            .process(|rows| rows.map(|rows| Ok(rows.process()?.detail)).transpose())
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
            .process(|rows| rows.map(ReleaseDetailRows::process_context).transpose())
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
            .subscribe_reconfigurable(initial, |release_id| {
                Ok(coven::Maybe::new(release_id.clone(), |release| ViewedReleaseDetailParams {
                    release: release.clone(),
                }))
            })
            .process(|_, rows| {
                let rows = rows
                    .map(|(_, rows)| at_most_one::<ViewedReleaseDetailParams, _>(rows))
                    .transpose()?
                    .flatten();
                let context = rows.map(|row| ReleaseDetailRows(row.viewed).process_context());
                Ok(ReleaseDetailProjection { context: context.transpose()? })
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
        cover: bae_records::cover_ref(
            release_id,
            detail.cover.as_ref().map(|cover| cover.version.clone()),
        ),
        is_compilation,
        album_year,
    };
    ReleaseDetail::from_raw(detail, download, actions, &ctx, tracks_rating_system)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
}

// A release as its detail view reads it: records' detail of it, with the
// stars this device's person gave its tracks, beside its album's compilation
// flag and year, its entry in the download queue, the storage actions it offers
// and how the library shows ratings.
coven::fragment!(
    ViewedRelease,
    r#"
    SELECT release.*, album.*, download.*, actions.*, systems.*
    FROM @bae_records::records::release_detail::ReleaseDetail(
        stars: @bae_people::people::fragments::OwnTrackStars) AS release
    JOIN @bae_records::records::release_detail::ReleaseAlbumFacts AS album
      ON album.album_id = release.album_id
    LEFT JOIN @bae_transfers::download_entry::QueuedDownloadEntry AS download
           ON download.release_id = release.detail_release_id
    JOIN @crate::storage_actions::ReleaseStorageActions AS actions
      ON actions.actions_release_id = release.detail_release_id
    JOIN @bae_people::people::rating_systems::LibraryRatingSystems AS systems ON 1
"#
);

// [`ViewedRelease`] of the release `release`; no row for a release the
// library does not hold.
coven::read!(
    writer ViewedReleaseDetail(release: String),
    r#"
    SELECT viewed.*
    FROM @crate::release::ViewedRelease AS viewed
    WHERE viewed.detail_release_id = :release
"#
);

// [`ViewedRelease`] of each release of the album `album`, in the album's
// order.
coven::read!(
    writer ViewedAlbumReleases(album: String),
    r#"
    SELECT viewed.*
    FROM @crate::release::ViewedRelease AS viewed
    WHERE viewed.album_id = :album
    ORDER BY viewed.album_order, viewed.detail_release_id
"#
);

/// The release's detail rows beside its album's compilation flag and year;
/// `None` when the release doesn't exist.
pub(crate) fn find_release_detail_context_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseDetailRows>, DbError> {
    let statement = ViewedReleaseDetailParams { release: release_id.to_owned() };
    Ok(statement_optional(sql, &statement)?.map(|row| ReleaseDetailRows(row.viewed)))
}

/// The read of the detail rows of each release of the album `album_id`, in
/// the album's order, in one statement.
pub(crate) fn album_releases_read(album_id: &str) -> ViewedAlbumReleasesParams {
    ViewedAlbumReleasesParams { album: album_id.to_owned() }
}

/// The detail rows of the releases [`album_releases_read`] returned.
pub(crate) fn album_releases_of(rows: Vec<ViewedAlbumReleasesRow>) -> Vec<ReleaseDetailRows> {
    rows.into_iter().map(|row| ReleaseDetailRows(row.viewed)).collect()
}

/// One release's [`ViewedRelease`] row.
#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseDetailRows(ViewedRelease);

impl ReleaseDetailRows {
    /// The rows read into the release's values.
    pub(crate) fn process(self) -> Result<ReleaseDetailRead, DbError> {
        let ViewedRelease { release, download, actions, systems, .. } = self.0;
        Ok(ReleaseDetailRead {
            detail: release.decoded()?,
            download: download.as_ref().map(|queued| queued.entry()).transpose()?,
            actions: actions.actions(),
            tracks_rating_system: systems.track_system()?,
        })
    }

    /// The rows read into the release's detail context.
    pub(crate) fn process_context(self) -> Result<ReleaseDetailContext, DbError> {
        let is_compilation = self.0.album.is_compilation();
        let album_year = self.0.album.year()?;
        let ReleaseDetailRead { detail, download, actions, tracks_rating_system } =
            self.process()?;
        Ok(ReleaseDetailContext {
            detail,
            download,
            actions,
            is_compilation,
            album_year,
            tracks_rating_system,
        })
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
