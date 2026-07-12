//! Album domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Get all albums in the library, sorted by the given criteria.
    ///
    /// Pass an empty slice for default sort (newest first).
    pub async fn get_albums(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
    ) -> Result<Vec<DbAlbum>, LibraryError> {
        Ok(self.database.get_albums(sort).await?)
    }

    /// Get a page of albums for lazy loading.
    pub async fn get_album_page(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<AlbumSummary>, LibraryError> {
        let raws = self.database.get_album_page(sort, offset, limit).await?;
        let release_ids: Vec<String> = raws
            .iter()
            .flat_map(|r| r.release_ids.iter().cloned())
            .collect();
        let covers = self.cover_refs(&release_ids).await?;
        Ok(raws
            .into_iter()
            .map(|raw| AlbumSummary::from_raw(raw, |rid| covers.get(rid).cloned()))
            .collect())
    }

    /// Resolve an album's 0-based position under a sort, matching the paging
    /// order of `get_album_page`. `None` if the album isn't in the library.
    pub async fn get_album_index(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        album_id: &str,
    ) -> Result<Option<u64>, LibraryError> {
        Ok(self.database.get_album_index(sort, album_id).await?)
    }

    /// Count total albums.
    pub async fn get_album_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_album_count().await?)
    }

    /// Get album by ID. Only a test helper — production reads albums through
    /// `find_album_detail` / `get_album_page`, never a bare row.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_album_by_id(&self, album_id: &str) -> Result<Option<DbAlbum>, LibraryError> {
        Ok(self.database.find_album_by_id(album_id).await?)
    }

    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<AlbumDetail>, LibraryError> {
        let Some(raw) = self.database.find_album_detail(album_id).await? else {
            return Ok(None);
        };
        Ok(Some(self.resolve_album_detail(raw).await?))
    }

    /// Search across albums and tracks.
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchResults, LibraryError> {
        let raw = self.database.search_library(query, limit).await?;
        let primary_ids: Vec<String> = raw
            .albums
            .iter()
            .filter_map(|a| a.primary_release_id.clone())
            .collect();
        let artist_ids: Vec<String> = raw.composers.iter().map(|c| c.artist.id.clone()).collect();
        let work_release_ids: Vec<String> = raw
            .works
            .iter()
            .filter_map(|w| w.representative_release_id.clone())
            .collect();
        let album_covers = self.cover_refs(&primary_ids).await?;
        let composer_images = self.artist_image_refs(&artist_ids).await?;
        let work_covers = self.cover_refs(&work_release_ids).await?;
        Ok(SearchResults::from_raw(
            raw,
            &album_covers,
            &composer_images,
            &work_covers,
        ))
    }

    /// Delete an album and all its associated data. The database rows are
    /// removed in one cleanup-aware transaction, then coven evicts the blobs
    /// named by the delete plans.
    pub async fn delete_album(&self, album_id: &str) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;

        // Collect track IDs from all releases before deletion for playback cleanup
        let mut all_track_ids = Vec::new();
        let mut db_cleanups = Vec::new();
        let mut evict_blobs = Vec::new();
        for release in &releases {
            let tracks = self.get_tracks_for_release(&release.id).await?;
            all_track_ids.extend(tracks.into_iter().map(|t| t.id));
            let delete_plan = self.release_delete_plan(release).await?;
            db_cleanups.push(delete_plan.db_cleanup);
            evict_blobs.extend(delete_plan.evict_blobs);
        }

        self.database
            .delete_album_with_cleanup(album_id, db_cleanups)
            .await?;
        self.emit_outbox_changed().await;
        self.evict_delete_blobs(evict_blobs).await;

        if !all_track_ids.is_empty() {
            self.emit(LibraryEvent::TracksDeleted {
                track_ids: all_track_ids,
            });
        }

        self.emit_album_removed(album_id, releases.iter().map(|r| r.id.clone()).collect());

        Ok(())
    }
}

impl LibraryManager {
    /// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`.
    /// Joins artist names, formats labels, groups tracks by side, builds
    /// galleries, and applies the `primary_release_id` fallback. The
    /// database lookup filters out empty-release aggregates, but callers with
    /// an already-read raw detail can still see an album whose releases were
    /// removed before resolution.
    pub(super) async fn resolve_album_detail(
        &self,
        raw: crate::db::DbAlbumDetail,
    ) -> Result<AlbumDetail, LibraryError> {
        let artist_names = join_artist_names(&raw.artists);
        let primary_release_id = crate::db::resolve_primary_release_id(
            raw.album.primary_release_id.as_deref(),
            raw.releases
                .iter()
                .map(|release| release.release.id.as_str()),
        )
        .ok_or_else(|| {
            LibraryError::TrackMapping(format!("Album '{}' has no releases", raw.album.id))
        })?;

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
                transfer_action: self.current_transfer_action(&r.release.id),
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
