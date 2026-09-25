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

    pub(crate) fn subscribe_album_browse(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        crate::db::AlbumBrowseProjection,
    > {
        self.database.subscribe_album_browse(sort, initial_windows)
    }

    pub(crate) fn resolve_album_browse(
        &self,
        projection: crate::db::AlbumBrowseProjection,
        request_revision: u64,
        cause: coven::ReconfigurableLiveQueryCause,
    ) -> crate::library::LibraryBrowseSnapshot<AlbumSummary> {
        let covers = image_refs(projection.cover_versions, LibraryImageType::Cover);
        crate::library::LibraryBrowseSnapshot {
            windows: projection
                .windows
                .into_iter()
                .map(|window| crate::library::LibraryBrowseWindow {
                    window: window.window,
                    rows: resolve_album_rows(window.rows, &covers),
                })
                .collect(),
            total_count: projection.total_count,
            request_revision,
            cause,
        }
    }

    pub(crate) fn subscribe_album_selection(
        &self,
        initial: std::collections::BTreeSet<String>,
    ) -> coven::ReconfigurableLiveQuery<
        std::collections::BTreeSet<String>,
        crate::db::AlbumSelectionProjection,
    > {
        self.database.subscribe_album_selection(initial)
    }

    pub(crate) fn resolve_album_selection(
        &self,
        projection: crate::db::AlbumSelectionProjection,
    ) -> Vec<AlbumSummary> {
        let covers = image_refs(projection.cover_versions, LibraryImageType::Cover);
        resolve_album_rows(projection.albums, &covers)
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

    pub(crate) fn subscribe_album_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, crate::db::AlbumDetailProjection> {
        self.database.subscribe_album_detail(initial)
    }

    /// Each release's representative file id, in release order: what the
    /// album's pin markers are watched by.
    pub(crate) fn album_detail_pin_files(
        projection: &crate::db::AlbumDetailProjection,
    ) -> Vec<Option<String>> {
        projection
            .detail
            .as_ref()
            .map(|raw| {
                album_pin_files(raw)
                    .map(|id| id.map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The ids of the releases an album-detail delivery shows.
    pub(crate) fn album_detail_release_ids(
        projection: &crate::db::AlbumDetailProjection,
    ) -> Vec<String> {
        projection
            .detail
            .as_ref()
            .map(|raw| {
                raw.releases
                    .iter()
                    .map(|release| release.release.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Resolve an album-detail delivery with its releases' pin markers, one per
    /// release in order (see [`Self::album_detail_pin_files`]).
    pub(crate) fn resolve_album_detail_projection(
        &self,
        projection: crate::db::AlbumDetailProjection,
        pinned: Vec<bool>,
    ) -> Result<Option<AlbumDetail>, LibraryError> {
        let Some(raw) = projection.detail else {
            return Ok(None);
        };
        let covers = image_refs(projection.cover_versions, LibraryImageType::Cover);
        self.resolve_album_detail_with_covers(raw, covers, pinned)
            .map(Some)
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

    pub(crate) fn subscribe_library_search(
        &self,
        initial: Option<crate::library::LibrarySearchQuery>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::library::LibrarySearchQuery>,
        crate::db::LibrarySearchProjection,
    > {
        self.database
            .subscribe_library_search(initial, crate::library::SEARCH_RESULT_LIMIT)
    }

    pub(crate) fn resolve_library_search_projection(
        &self,
        projection: crate::db::LibrarySearchProjection,
    ) -> SearchResults {
        let covers = image_refs(projection.cover_versions, LibraryImageType::Cover);
        let artist_images = image_refs(projection.artist_image_versions, LibraryImageType::Artist);
        SearchResults::from_raw(projection.results, &covers, &artist_images)
    }

    /// Delete an album and its data: the rows go in one transaction, which
    /// declares every release's blobs deleted so coven reclaims their on-device
    /// copies with the commit.
    pub async fn delete_album(&self, album_id: &str) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;

        // Read every release's track ids before the delete cascades them away —
        // playback needs them to clear the queue.
        let mut all_track_ids = Vec::new();
        let mut deletions = Vec::new();
        for release in &releases {
            let tracks = self.get_tracks_for_release(&release.id).await?;
            all_track_ids.extend(tracks.into_iter().map(|t| t.id));
            // See `delete_release`: the delete records each release's unwind in
            // the transaction that removes it.
            deletions.push(self.database.plan_release_deletion(&release.id).await?);
        }

        self.database
            .delete_album_with_cleanup(album_id, deletions)
            .await?;
        self.emit_outbox_changed().await;

        if !all_track_ids.is_empty() {
            self.emit(LibraryEvent::TracksDeleted {
                track_ids: all_track_ids,
            });
        }

        Ok(())
    }
}

/// Each release's representative file — the one its pin marker is asked by —
/// in release order.
fn album_pin_files(raw: &crate::db::DbAlbumDetail) -> impl Iterator<Item = Option<&str>> {
    raw.releases
        .iter()
        .map(|release| release.files.first().map(|file| file.id.as_str()))
}

fn resolve_album_rows(
    rows: Vec<crate::db::DbAlbumSummary>,
    covers: &HashMap<String, ImageRef>,
) -> Vec<AlbumSummary> {
    rows.into_iter()
        .map(|row| AlbumSummary::from_raw(row, |id| covers.get(id).cloned()))
        .collect()
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
        let release_ids: Vec<String> = raw.releases.iter().map(|r| r.release.id.clone()).collect();
        let covers = self.cover_refs(&release_ids).await?;
        let pinned = self
            .releases_pinned(&album_pin_files(&raw).collect::<Vec<_>>())
            .await?;
        self.resolve_album_detail_with_covers(raw, covers, pinned)
    }

    fn resolve_album_detail_with_covers(
        &self,
        raw: crate::db::DbAlbumDetail,
        covers: HashMap<String, ImageRef>,
        pin_states: Vec<bool>,
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
        let cover = covers.get(&primary_release_id).cloned();
        let mut releases = Vec::with_capacity(raw.releases.len());
        for (i, (r, pinned)) in raw.releases.into_iter().zip(pin_states).enumerate() {
            let release_cover = covers.get(&r.release.id).cloned();
            let ctx = ReleaseResolveCtx {
                has_cloud_home,
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
