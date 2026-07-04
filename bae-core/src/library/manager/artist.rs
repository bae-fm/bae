//! Artist domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Insert an artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }

    /// Get artist by Discogs ID (for deduplication)
    pub async fn get_artist_by_discogs_id(
        &self,
        discogs_artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self
            .database
            .get_artist_by_discogs_id(discogs_artist_id)
            .await?)
    }

    /// Get artist by MusicBrainz ID (for deduplication)
    pub async fn get_artist_by_mb_id(&self, mb_id: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_mb_id(mb_id).await?)
    }

    /// Get artist by name (case-insensitive, first match)
    pub async fn get_artist_by_name(&self, name: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_name(name).await?)
    }

    /// Fill in NULL external IDs on an existing artist (never overwrites)
    pub async fn update_artist_external_ids(
        &self,
        id: &str,
        discogs_id: Option<&str>,
        mb_id: Option<&str>,
        sort_name: Option<&str>,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .update_artist_external_ids(id, discogs_id, mb_id, sort_name)
            .await?)
    }

    /// Insert album-artist relationship
    pub async fn insert_album_artist(
        &self,
        album_artist: &DbAlbumArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_album_artist(album_artist).await?;
        Ok(())
    }

    /// Insert track-artist relationship
    pub async fn insert_track_artist(
        &self,
        track_artist: &DbTrackArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_track_artist(track_artist).await?;
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

    /// Resolve each parsed artist to an existing DB row or insert a new one.
    ///
    /// Returns the DB artist ID for each input in the same order, so callers can
    /// zip with `artists` to build a parsed-ID -> DB-ID map.
    ///
    /// Lookup chain: Various Artists alias (cross-source), `discogs_artist_id`,
    /// `musicbrainz_artist_id`, name (case-insensitive) with source-ID conflict
    /// check, then insert. On a match, any new source IDs are accumulated onto
    /// the existing row via COALESCE.
    pub async fn find_or_create_artists(
        &self,
        artists: &[DbArtist],
    ) -> Result<Vec<String>, LibraryError> {
        let mut resolved = Vec::with_capacity(artists.len());

        for artist in artists {
            let existing = self.find_existing_artist_for_import(artist).await?;
            let actual_id = if let Some(existing_artist) = existing {
                self.database
                    .update_artist_external_ids(
                        &existing_artist.id,
                        artist.discogs_artist_id.as_deref(),
                        artist.musicbrainz_artist_id.as_deref(),
                        artist.sort_name.as_deref(),
                    )
                    .await?;
                existing_artist.id
            } else {
                self.database.insert_artist(artist).await?;
                artist.id.clone()
            };

            resolved.push(actual_id);
        }

        Ok(resolved)
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
