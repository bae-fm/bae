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
            // 0. Various Artists: match any known VA ID across sources so that
            //    e.g. Discogs "Various" (ID 194) merges with MB "Various Artists".
            let existing = if artist.is_various_artists() {
                let va = &crate::db::VARIOUS_ARTISTS;
                let by_discogs = self.database.get_artist_by_discogs_id(va.discogs).await?;

                if by_discogs.is_some() {
                    by_discogs
                } else {
                    self.database.get_artist_by_mb_id(va.musicbrainz).await?
                }
            } else {
                None
            };

            // 1. Try discogs_artist_id
            let existing = if existing.is_some() {
                existing
            } else if let Some(ref discogs_id) = artist.discogs_artist_id {
                self.database.get_artist_by_discogs_id(discogs_id).await?
            } else {
                None
            };

            // 2. Try musicbrainz_artist_id
            let existing = match existing {
                Some(e) => Some(e),
                None => {
                    if let Some(ref mb_id) = artist.musicbrainz_artist_id {
                        self.database.get_artist_by_mb_id(mb_id).await?
                    } else {
                        None
                    }
                }
            };

            // 3. Try name (case-insensitive) with conflict check
            let existing = match existing {
                Some(e) => Some(e),
                None => {
                    let name_match = self.database.get_artist_by_name(&artist.name).await?;

                    match name_match {
                        Some(ref matched) => {
                            let discogs_conflict =
                                match (&matched.discogs_artist_id, &artist.discogs_artist_id) {
                                    (Some(a), Some(b)) => a != b,
                                    _ => false,
                                };
                            let mb_conflict = match (
                                &matched.musicbrainz_artist_id,
                                &artist.musicbrainz_artist_id,
                            ) {
                                (Some(a), Some(b)) => a != b,
                                _ => false,
                            };

                            if discogs_conflict || mb_conflict {
                                debug!(
                                    "Name match for '{}' has conflicting source IDs, inserting new artist",
                                    artist.name
                                );
                                None
                            } else {
                                name_match
                            }
                        }
                        None => None,
                    }
                }
            };

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
}
