//! Artist domain operations for [`LibraryManager`].

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedImportArtists {
    pub ids: Vec<String>,
    pub inserts: Vec<DbArtist>,
    pub external_id_updates: Vec<(String, DbArtist)>,
}

impl LibraryManager {
    /// Insert an artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }

    /// Get artists for an album
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }

    /// Get artists for a track
    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_track(track_id).await?)
    }

    /// Get artist by ID
    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.find_artist_by_id(artist_id).await?)
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

    /// Resolve each parsed artist to an existing DB row or a row to insert at
    /// finalize.
    ///
    /// Returns the DB artist ID for each input in the same order, so callers can
    /// zip with `artists` to build a parsed-ID -> DB-ID map.
    ///
    /// Lookup chain: Various Artists alias (cross-source), `discogs_artist_id`,
    /// `musicbrainz_artist_id`, name (case-insensitive) with source-ID
    /// conflict check, then deferred insert. On a match, any new source IDs are
    /// carried as a deferred COALESCE update.
    pub(crate) async fn resolve_artists_for_import(
        &self,
        artists: &[DbArtist],
    ) -> Result<ResolvedImportArtists, LibraryError> {
        let mut ids = Vec::with_capacity(artists.len());
        let mut inserts = Vec::new();
        let mut external_id_updates = Vec::new();

        for artist in artists {
            let existing = self.find_existing_artist_for_import(artist).await?;
            let actual_id = if let Some(existing_artist) = existing {
                let id = existing_artist.id;
                external_id_updates.push((id.clone(), artist.clone()));
                id
            } else {
                inserts.push(artist.clone());
                artist.id.clone()
            };

            ids.push(actual_id);
        }

        Ok(ResolvedImportArtists {
            ids,
            inserts,
            external_id_updates,
        })
    }

    /// Resolve each parsed artist to an existing DB row or insert it
    /// immediately. Used by metadata edits, whose DB write path updates an
    /// already-finalized release.
    pub async fn find_or_create_artists(
        &self,
        artists: &[DbArtist],
    ) -> Result<Vec<String>, LibraryError> {
        let resolved = self.resolve_artists_for_import(artists).await?;
        for artist in &resolved.inserts {
            self.database.insert_artist(artist).await?;
        }
        for (artist_id, artist) in &resolved.external_id_updates {
            self.database
                .update_artist_external_ids(
                    artist_id,
                    artist.discogs_artist_id.as_deref(),
                    artist.musicbrainz_artist_id.as_deref(),
                    artist.sort_name.as_deref(),
                )
                .await?;
        }
        Ok(resolved.ids)
    }

    async fn find_existing_artist_for_import(
        &self,
        artist: &DbArtist,
    ) -> Result<Option<DbArtist>, LibraryError> {
        // Various Artists: match any known VA ID across sources so that
        // Discogs "Various" merges with MusicBrainz "Various Artists".
        if artist.is_various_artists() {
            let va = &crate::db::VARIOUS_ARTISTS;
            if let Some(existing) = self.database.get_artist_by_discogs_id(va.discogs).await? {
                return Ok(Some(existing));
            }
            if let Some(existing) = self.database.get_artist_by_mb_id(va.musicbrainz).await? {
                return Ok(Some(existing));
            }
        }

        if let Some(discogs_id) = artist.discogs_artist_id.as_deref() {
            if let Some(existing) = self.database.get_artist_by_discogs_id(discogs_id).await? {
                return Ok(Some(existing));
            }
        }

        if let Some(mb_id) = artist.musicbrainz_artist_id.as_deref() {
            if let Some(existing) = self.database.get_artist_by_mb_id(mb_id).await? {
                return Ok(Some(existing));
            }
        }

        self.find_name_match_for_import(artist).await
    }

    async fn find_name_match_for_import(
        &self,
        artist: &DbArtist,
    ) -> Result<Option<DbArtist>, LibraryError> {
        let Some(matched) = self.database.get_artist_by_name(&artist.name).await? else {
            return Ok(None);
        };

        if source_id_conflicts(
            matched.discogs_artist_id.as_deref(),
            artist.discogs_artist_id.as_deref(),
        ) || source_id_conflicts(
            matched.musicbrainz_artist_id.as_deref(),
            artist.musicbrainz_artist_id.as_deref(),
        ) {
            debug!(
                "Name match for '{}' has conflicting source IDs, inserting new artist",
                artist.name
            );
            Ok(None)
        } else {
            Ok(Some(matched))
        }
    }
}

fn source_id_conflicts(existing: Option<&str>, incoming: Option<&str>) -> bool {
    matches!((existing, incoming), (Some(a), Some(b)) if a != b)
}
