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
        Ok(raws.into_iter().map(AlbumSummary::from_raw).collect())
    }

    /// Resolve the album's first appearance under the active browse order.
    /// `None` if the album isn't in the library.
    pub async fn get_album_index(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        album_id: &str,
        group_by_artist: bool,
    ) -> Result<Option<u64>, LibraryError> {
        Ok(self
            .database
            .get_album_index(sort, album_id, group_by_artist)
            .await?)
    }

    /// The albums of `album_ids` the grid lists, in its order under `sort`
    /// and `group_by_artist`, each once.
    pub async fn albums_in_browse_order(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        album_ids: Vec<String>,
        group_by_artist: bool,
    ) -> Result<Vec<AlbumSummary>, LibraryError> {
        let raws = self
            .database
            .albums_in_browse_order(sort, album_ids, group_by_artist)
            .await?;
        Ok(raws.into_iter().map(AlbumSummary::from_raw).collect())
    }

    pub async fn get_album_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_album_count().await?)
    }

    pub(crate) fn subscribe_album_browse(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        group_by_artist: bool,
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        crate::db::AlbumBrowseProjection,
    > {
        self.database
            .subscribe_album_browse(sort, group_by_artist, initial_windows)
    }

    pub(crate) fn resolve_album_browse(
        &self,
        projection: crate::db::AlbumBrowseProjection,
        request_revision: u64,
        cause: coven::LiveQueryCause,
    ) -> crate::library::LibraryBrowseSnapshot<AlbumSummary> {
        crate::library::LibraryBrowseSnapshot {
            windows: projection
                .windows
                .into_iter()
                .map(|window| crate::library::LibraryBrowseWindow {
                    window: window.window,
                    rows: window
                        .rows
                        .into_iter()
                        .map(AlbumSummary::from_raw)
                        .collect(),
                })
                .collect(),
            total_count: projection.total_count,
            row_count: projection.row_count,
            sections: projection.sections,
            request_revision,
            cause,
        }
    }

    pub(crate) fn subscribe_album_selection(
        &self,
        initial: std::collections::BTreeSet<String>,
    ) -> coven::ReconfigurableLiveQuery<std::collections::BTreeSet<String>, Vec<String>> {
        self.database.subscribe_album_selection(initial)
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

    /// The originals the library records this device holding for the
    /// releases an album-detail delivery shows: what the disk is asked about.
    pub(crate) fn album_detail_originals(
        projection: &crate::db::AlbumDetailProjection,
    ) -> Vec<crate::db::RecordedOriginal> {
        projection
            .detail
            .as_ref()
            .map(|raw| {
                crate::album_detail::recorded_originals(
                    raw.releases
                        .iter()
                        .flat_map(|release| &release.unstored_audio),
                )
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

    /// Resolve an album-detail delivery with what the disk has said of its
    /// originals (see [`Self::album_detail_originals`]).
    pub(crate) fn resolve_album_detail_projection(
        &self,
        projection: crate::db::AlbumDetailProjection,
        originals: &crate::album_detail::OriginalsSeen,
    ) -> Result<Option<AlbumDetail>, LibraryError> {
        let Some(raw) = projection.detail else {
            return Ok(None);
        };
        self.resolve_album_detail_with(raw, originals).map(Some)
    }

    /// The library albums a parsed, non-blank query finds, as the album
    /// picker lists them: title, the album's credit line and year.
    #[cfg(desktop)]
    pub async fn search_albums(
        &self,
        query: &crate::library::LibrarySearchQuery,
    ) -> Result<Vec<crate::db::DbAlbumSearchResult>, LibraryError> {
        Ok(self
            .database
            .search_library(query.as_str(), crate::library::SEARCH_RESULT_LIMIT)
            .await?
            .albums)
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
        Ok(SearchResults::from_raw(raw))
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
        SearchResults::from_raw(projection.results)
    }

    /// Delete an album: every release it holds goes, with what each owns, in
    /// one transaction, as [`Self::delete_release`] deletes one. Each coven
    /// file of theirs goes with its last reference; the album row stays,
    /// empty, for the reason in `vacate_album_on`. An album holding no
    /// release is not in the library, and deleting it is refused.
    pub async fn delete_album(&self, album_id: &str) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;
        if releases.is_empty() {
            return Err(LibraryError::Refused(format!(
                "Album not found: {album_id}"
            )));
        }

        // Read every release's track ids before the delete cascades them away —
        // the player stops a deleted track it plays or has loaded next. The
        // queue's rows leave in the delete's own transaction.
        let mut all_track_ids = Vec::new();
        let mut deletions = Vec::new();
        for release in &releases {
            let tracks = self.get_tracks_for_release(&release.id).await?;
            all_track_ids.extend(tracks.into_iter().map(|t| t.id));
            // Each plan names the rows and coven files it was read against;
            // the write refuses a release that changed since.
            deletions.push(self.database.plan_release_deletion(&release.id).await?);
        }

        self.database
            .delete_album_with_cleanup(album_id, deletions)
            .await?;

        if !all_track_ids.is_empty() {
            self.tracks_deleted(all_track_ids);
        }

        Ok(())
    }
}

impl LibraryManager {
    /// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`: joins
    /// artist names, formats labels, groups tracks by side, and builds
    /// galleries.
    pub(super) async fn resolve_album_detail(
        &self,
        raw: crate::db::DbAlbumDetail,
    ) -> Result<AlbumDetail, LibraryError> {
        let originals = self
            .look_at_originals(crate::album_detail::recorded_originals(
                raw.releases
                    .iter()
                    .flat_map(|release| &release.unstored_audio),
            ))
            .await?;
        self.resolve_album_detail_with(raw, &originals)
    }

    /// The album detail as its rows read it, with what the disk said of its
    /// originals.
    fn resolve_album_detail_with(
        &self,
        raw: crate::db::DbAlbumDetail,
        originals: &crate::album_detail::OriginalsSeen,
    ) -> Result<AlbumDetail, LibraryError> {
        let artist_names = crate::credits::library_line(&raw.artists);
        let primary_release_id = raw.primary_release_id;

        let has_cloud_home = self.has_cloud_home();
        let cover = crate::album_detail::cover_ref(&primary_release_id, raw.primary_cover_version);
        let mut releases = Vec::with_capacity(raw.releases.len());
        for (i, r) in raw.releases.into_iter().enumerate() {
            let ctx = ReleaseResolveCtx {
                has_cloud_home,
                originals,
                cover: crate::album_detail::cover_ref(&r.release.id, r.cover_version.clone()),
                transfer_action: self.current_transfer_action(&r.release.id),
                is_compilation: raw.album.is_compilation,
            };
            let (detail, orphans) = ReleaseDetail::from_raw(r, &raw.artists, i, &ctx);
            self.report_audio_format_orphans(orphans);
            releases.push(detail);
        }

        Ok(AlbumDetail {
            album: raw.album,
            artists: raw.artists,
            artist_names,
            releases,
            primary_release_id,
            cover,
        })
    }
}
