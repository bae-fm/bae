//! Artist domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }

    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<crate::credits::LibraryCredit>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }

    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<crate::credits::LibraryCredit>, LibraryError> {
        Ok(self.database.get_artists_for_track(track_id).await?)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.find_artist_by_id(artist_id).await?)
    }

    #[cfg(desktop)]
    pub(crate) async fn choose_import_artist_identity(
        &self,
        content_hash: &str,
        metadata_revision: u64,
        choice: crate::import::ArtistIdentityConflictChoice,
    ) -> Result<(), LibraryError> {
        self.database
            .choose_import_artist_identity(
                content_hash,
                metadata_revision,
                choice,
                self.ids.as_ref(),
            )
            .await?;
        Ok(())
    }

    /// Follow what an artist picker's search finds, from `initial` on.
    pub(crate) fn subscribe_artist_search(
        &self,
        initial: crate::library::ArtistSearchView,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::ArtistSearchView,
        crate::db::ArtistSearchProjection,
    > {
        self.database.subscribe_artist_search(initial)
    }

    /// The artists an artist search's windows hold, each beside its image
    /// and its other printed names.
    pub(crate) fn resolve_artist_search(
        projection: crate::db::ArtistSearchProjection,
        request_revision: u64,
    ) -> crate::library::ArtistSearchSnapshot {
        let crate::db::ArtistSearchProjection {
            windows,
            total_count,
        } = projection;
        crate::library::ArtistSearchSnapshot {
            windows: windows
                .into_iter()
                .map(|window| crate::library::LibraryBrowseWindow {
                    window: window.window,
                    rows: window
                        .rows
                        .into_iter()
                        .map(|row| ArtistSearchResult {
                            image: crate::album_detail::artist_image_ref(
                                &row.artist.id,
                                row.image_version,
                            ),
                            credit_names: row.credit_names,
                            artist: row.artist,
                        })
                        .collect(),
                })
                .collect(),
            total_count,
            request_revision,
        }
    }

    /// Give an artist another printed name, which credits can then print for
    /// it. A name it already goes by adds nothing.
    pub async fn add_artist_credit_name(
        &self,
        artist_id: &str,
        name: &str,
    ) -> Result<(), LibraryError> {
        if name.trim().is_empty() {
            return Err(LibraryError::Refused(
                "a credit name cannot be blank".into(),
            ));
        }
        Ok(self
            .database
            .add_artist_credit_name(artist_id, name)
            .await?)
    }

    /// Rename one of an artist's other printed names; every credit printing
    /// it prints the new name.
    pub async fn rename_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
        name: &str,
    ) -> Result<(), LibraryError> {
        if name.trim().is_empty() {
            return Err(LibraryError::Refused(
                "a credit name cannot be blank".into(),
            ));
        }
        Ok(self
            .database
            .rename_artist_credit_name(artist_id, key, name)
            .await?)
    }

    /// Remove one of an artist's other printed names; every credit printing
    /// it prints the artist's own name.
    pub async fn remove_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .remove_artist_credit_name(artist_id, key)
            .await?)
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
        Ok(raw.into_iter().map(ArtistSummary::from_raw).collect())
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
        cause: coven::LiveQueryCause,
    ) -> crate::library::LibraryBrowseSnapshot<ArtistSummary> {
        crate::library::LibraryBrowseSnapshot {
            windows: projection
                .windows
                .into_iter()
                .map(|window| crate::library::LibraryBrowseWindow {
                    window: window.window,
                    rows: window
                        .rows
                        .into_iter()
                        .map(ArtistSummary::from_raw)
                        .collect(),
                })
                .collect(),
            total_count: projection.total_count,
            row_count: projection.total_count,
            sections: Vec::new(),
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
        let albums = raw.albums.into_iter().map(AlbumSummary::from_raw).collect();
        Ok(Some(ArtistDetail {
            artist: ArtistSummary::from_raw(raw.artist),
            credit_names: raw.credit_names,
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
        Some(ArtistDetail {
            artist: ArtistSummary::from_raw(raw.artist),
            credit_names: raw.credit_names,
            albums: raw.albums.into_iter().map(AlbumSummary::from_raw).collect(),
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
