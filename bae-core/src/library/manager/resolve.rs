//! I/O resolvers: read the `&Database` / `&CovenHandle` to gather the inputs
//! (covers, pin state, cloud-home presence, album/release joins) the pure
//! projections in `crate::album_detail` need, then hand a raw `Db*` aggregate
//! plus those inputs to the produced type's `from_raw` constructor.

use super::*;

/// The cover [`ImageRef`] for one release from its `covers` row's `_updated_at`,
/// or `None` when it has no cover row. Free function so the manager's `cover_ref`
/// and the observer's `find_release_detail_with` share one construction.
pub(crate) async fn cover_ref_for(
    database: &Database,
    release_id: &str,
) -> Result<Option<ImageRef>, LibraryError> {
    Ok(database
        .cover_version(release_id)
        .await?
        .map(|version| ImageRef {
            id: release_id.to_string(),
            version,
        }))
}

/// Free-function variant of `LibraryManager::find_release_detail`.
///
/// Used by the manager and by the upload observer (which holds the same
/// `Database` and a `CovenHandle` so it can emit `ReleaseUpdated` events for a
/// release whose `local_path` just got cleared at the end of an upload run,
/// without owning a manager). The pin-state is answered through `handle`, the
/// same door the manager uses. `has_cloud_home` is supplied by the caller; the
/// observer fires inside a running sync cycle so it can pass `true`.
pub(crate) async fn find_release_detail_with(
    database: &Database,
    handle: &CovenHandle,
    has_cloud_home: bool,
    release_id: &str,
) -> Result<Option<ReleaseDetail>, LibraryError> {
    let Some(raw) = database.find_release_detail(release_id).await? else {
        return Ok(None);
    };
    let album_id = raw.release.album_id.clone();
    let album_artists = database.get_artists_for_album(&album_id).await?;
    let releases = database.get_releases_for_album(&album_id).await?;
    let release_index = releases
        .iter()
        .position(|r| r.id == release_id)
        .expect("release belongs to its album");
    let pinned = match raw.files.first() {
        Some(file) => release_file_pinned(handle, &file.id).await?,
        None => false,
    };
    let cover = cover_ref_for(database, release_id).await?;
    let ctx = ReleaseResolveCtx {
        has_cloud_home,
        pinned,
        cover,
    };
    Ok(Some(ReleaseDetail::from_raw(
        raw,
        &album_artists,
        release_index,
        &ctx,
    )))
}

impl LibraryManager {
    /// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`.
    /// Joins artist names, formats labels, groups tracks by side, builds
    /// galleries, and applies the `primary_release_id` fallback. The
    /// fallback always succeeds: every album has at least one release
    /// (enforced by `delete_release`).
    pub(super) async fn resolve_album_detail(
        &self,
        raw: crate::db::DbAlbumDetail,
    ) -> Result<AlbumDetail, LibraryError> {
        let artist_names = join_artist_names(&raw.artists);
        // The primary is the stored value when it's still present, otherwise
        // the album's first release.
        let primary_release_id = raw
            .album
            .primary_release_id
            .clone()
            .filter(|id| raw.releases.iter().any(|r| &r.release.id == id))
            .or_else(|| raw.releases.first().map(|r| r.release.id.clone()))
            .expect("album has at least one release");

        let has_cloud_home = self.has_cloud_home();
        // One cover lookup for the whole album: the album's cover is the primary
        // release's, and each release carries its own.
        let release_ids: Vec<String> = raw.releases.iter().map(|r| r.release.id.clone()).collect();
        let covers = self.cover_refs(&release_ids).await?;
        let cover = covers.get(&primary_release_id).cloned();
        let mut releases = Vec::with_capacity(raw.releases.len());
        for (i, r) in raw.releases.into_iter().enumerate() {
            // Ask coven's cache whether this release is pinned — the orthogonal
            // coven-cache property of a remote release.
            let pinned = self
                .release_pinned(r.files.first().map(|f| f.id.as_str()))
                .await?;
            let release_cover = covers.get(&r.release.id).cloned();
            let ctx = ReleaseResolveCtx {
                has_cloud_home,
                pinned,
                cover: release_cover,
            };
            releases.push(ReleaseDetail::from_raw(r, &raw.artists, i, &ctx));
        }

        Ok(AlbumDetail {
            album: raw.album,
            artist_names,
            releases,
            primary_release_id,
            cover,
        })
    }
}
