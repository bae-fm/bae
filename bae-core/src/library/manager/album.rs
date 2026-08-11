//! Album domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Every album, sorted by `sort` — an empty slice means newest first.
    pub async fn get_albums(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
    ) -> Result<Vec<DbAlbum>, LibraryError> {
        Ok(self.database.get_albums(sort).await?)
    }

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

    pub async fn get_album_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_album_count().await?)
    }

    /// Test-only. Production reads albums through `find_album_detail` /
    /// `get_album_page`, never as a bare row.
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

    /// Search the library for a parsed, non-blank query. The result count is the
    /// core-owned [`crate::library::SEARCH_RESULT_LIMIT`]; no caller passes one.
    pub async fn search_library(
        &self,
        query: &crate::library::LibrarySearchQuery,
    ) -> Result<SearchResults, LibraryError> {
        let raw = self
            .database
            .search_library(query.as_str(), crate::library::SEARCH_RESULT_LIMIT)
            .await?;
        // Prefetch every release cover the resolver looks up, in one batch:
        // each album's *resolved* primary release, each track's own release,
        // and each work's representative release — all keyed by release id,
        // the same ids `SearchResults::from_raw` reads back.
        let mut release_ids: Vec<String> = raw
            .albums
            .iter()
            .filter_map(|a| {
                crate::db::resolve_primary_release_id(
                    a.primary_release_id.as_deref(),
                    a.release_ids.iter().map(String::as_str),
                )
            })
            .collect();
        release_ids.extend(raw.tracks.iter().map(|t| t.release_id.clone()));
        release_ids.extend(
            raw.works
                .iter()
                .filter_map(|w| w.representative_release_id.clone()),
        );
        // Artist and composer hits share one image prefetch: both are artist
        // rows, keyed by artist id.
        let mut artist_ids: Vec<String> = raw.artists.iter().map(|a| a.artist.id.clone()).collect();
        artist_ids.extend(raw.composers.iter().map(|c| c.artist.id.clone()));
        let release_covers = self.cover_refs(&release_ids).await?;
        let artist_images = self.artist_image_refs(&artist_ids).await?;
        Ok(SearchResults::from_raw(
            raw,
            &release_covers,
            &artist_images,
        ))
    }

    /// Delete an album and its data: the rows go in one cleanup-aware transaction,
    /// then coven evicts the blobs the delete plans name.
    pub async fn delete_album(&self, album_id: &str) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;

        // Read every release's track ids before the delete cascades them away —
        // playback needs them to clear the queue.
        let mut all_track_ids = Vec::new();
        let mut db_cleanups = Vec::new();
        let mut evict_blobs = Vec::new();
        for release in &releases {
            let tracks = self.get_tracks_for_release(&release.id).await?;
            all_track_ids.extend(tracks.into_iter().map(|t| t.id));
            let delete_plan = self.release_delete_plan(release).await?;
            // See `delete_release`: a make-remote caught mid-flight is coven's to
            // unwind, before the rows it covers are removed.
            if delete_plan.cancel_make_remote {
                self.coven_cancel_make_remote(&release.id).await?;
            }
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
    /// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`: joins
    /// artist names, formats labels, groups tracks by side, builds galleries, and
    /// applies the `primary_release_id` fallback. Errors on an album with no
    /// releases — the DB lookup filters those out, but a caller holding an
    /// already-read raw detail can still reach one whose releases were removed since.
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
        let sync_ready = self.is_sync_ready();
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
                sync_ready,
                pinned,
                cover: release_cover,
                transfer_action: self.current_transfer_action(&r.release.id),
                is_compilation: raw.album.is_compilation,
            };
            let (detail, orphans) = ReleaseDetail::from_raw(r, &raw.artists, i, &ctx);
            self.report_audio_format_orphans(orphans);
            releases.push(detail);
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
