//! Artist domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }

    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }

    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_track(track_id).await?)
    }

    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.find_artist_by_id(artist_id).await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn merge_import_artist_identity_conflict(
        &self,
        content_hash: &str,
        surviving_artist_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .merge_import_artist_identity_conflict(content_hash, surviving_artist_id)
            .await?;
        Ok(())
    }

    /// Search existing artists by library ID, provider ID, display name, or
    /// sort name. The parsed query and result limit are shared with library
    /// search, while this result set includes artists without album links.
    pub async fn search_artists(
        &self,
        query: &crate::library::LibrarySearchQuery,
    ) -> Result<Vec<ArtistSearchResult>, LibraryError> {
        let artists = self
            .database
            .search_artists(query.as_str(), crate::library::SEARCH_RESULT_LIMIT)
            .await?;
        let artist_ids = artists
            .iter()
            .map(|artist| artist.id.clone())
            .collect::<Vec<_>>();
        let images = self.artist_image_refs(&artist_ids).await?;
        Ok(artists
            .into_iter()
            .map(|artist| {
                let image = images.get(&artist.id).cloned();
                ArtistSearchResult { artist, image }
            })
            .collect())
    }

    pub async fn get_artist_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_artist_count().await?)
    }

    pub async fn get_artist_page(
        &self,
        sort: &[crate::db::ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<ArtistSummary>, LibraryError> {
        let raw = self.database.get_artist_page(sort, offset, limit).await?;
        let artist_ids: Vec<String> = raw.iter().map(|a| a.artist.id.clone()).collect();
        let images = self.artist_image_refs(&artist_ids).await?;
        Ok(raw
            .into_iter()
            .map(|artist| {
                let image = images.get(&artist.artist.id).cloned();
                ArtistSummary::from_raw(artist, image)
            })
            .collect())
    }

    pub(crate) fn subscribe_artist_browse(
        &self,
        sort: &[crate::db::ArtistSortCriterion],
        initial_windows: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        crate::db::ArtistBrowseProjection,
    > {
        self.database.subscribe_artist_browse(sort, initial_windows)
    }

    pub(crate) fn resolve_artist_browse(
        &self,
        projection: crate::db::ArtistBrowseProjection,
        request_revision: u64,
        cause: coven::ReconfigurableLiveQueryCause,
    ) -> crate::library::LibraryBrowseSnapshot<ArtistSummary> {
        let images = image_refs(projection.image_versions, LibraryImageType::Artist);
        crate::library::LibraryBrowseSnapshot {
            windows: projection
                .windows
                .into_iter()
                .map(|window| crate::library::LibraryBrowseWindow {
                    window: window.window,
                    rows: window
                        .rows
                        .into_iter()
                        .map(|row| {
                            let image = images.get(&row.artist.id).cloned();
                            ArtistSummary::from_raw(row, image)
                        })
                        .collect(),
                })
                .collect(),
            total_count: projection.total_count,
            request_revision,
            cause,
        }
    }

    pub async fn get_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<ArtistDetail>, LibraryError> {
        let Some(raw) = self.database.find_artist_detail(artist_id).await? else {
            return Ok(None);
        };
        let images = self
            .artist_image_refs(std::slice::from_ref(&raw.artist.artist.id))
            .await?;
        let release_ids: Vec<String> = raw
            .albums
            .iter()
            .flat_map(|a| a.release_ids.iter().cloned())
            .collect();
        let covers = self.cover_refs(&release_ids).await?;
        let albums = raw
            .albums
            .into_iter()
            .map(|album| AlbumSummary::from_raw(album, |rid| covers.get(rid).cloned()))
            .collect();
        let image = images.get(&raw.artist.artist.id).cloned();
        Ok(Some(ArtistDetail {
            artist: ArtistSummary::from_raw(raw.artist, image),
            albums,
        }))
    }

    pub(crate) fn subscribe_artist_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, crate::db::ArtistDetailProjection> {
        self.database.subscribe_artist_detail(initial)
    }

    pub(crate) fn resolve_artist_detail_projection(
        &self,
        projection: crate::db::ArtistDetailProjection,
    ) -> Option<ArtistDetail> {
        let raw = projection.detail?;
        let images = image_refs(projection.image_versions, LibraryImageType::Artist);
        let covers = image_refs(projection.cover_versions, LibraryImageType::Cover);
        let image = images.get(&raw.artist.artist.id).cloned();
        Some(ArtistDetail {
            artist: ArtistSummary::from_raw(raw.artist, image),
            albums: raw
                .albums
                .into_iter()
                .map(|album| AlbumSummary::from_raw(album, |id| covers.get(id).cloned()))
                .collect(),
        })
    }

    /// What the library holds, as it stands now, for every artist credit
    /// `edit` carries — how the release editor, which holds its form itself,
    /// shows the credits in it.
    pub async fn resolve_release_edit_credits(
        &self,
        edit: &crate::import::RawReleaseEdit,
    ) -> Result<Vec<crate::import::ResolvedCredit>, LibraryError> {
        let credits: Vec<_> = edit.credits().cloned().collect();
        Ok(self.database.resolve_artist_credits(&credits).await?)
    }

    /// Resolve `artists` as credits and write the new and filled-in rows, in
    /// one transaction, returning the artist each resolved to. The artist rule
    /// every import and edit commits by, for tests that need library artists
    /// without writing a release.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_or_create_artists(
        &self,
        artists: &[DbArtist],
    ) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.find_or_create_artists(artists).await?)
    }
}
