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
    ) -> Result<Vec<bae_db::credits::LibraryCredit>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }

    /// The credits the track shows: its own, else its album's.
    pub async fn get_shown_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<bae_db::credits::LibraryCredit>, LibraryError> {
        Ok(self.database.get_shown_artists_for_track(track_id).await?)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.find_artist_by_id(artist_id).await?)
    }

    /// Give an artist another printed name, which credits can then print for
    /// it. A name it already goes by adds nothing.
    pub async fn add_artist_credit_name(
        &self,
        artist_id: &str,
        name: &str,
    ) -> Result<(), LibraryError> {
        let name = credit_name(name)?;
        Ok(self.database.add_artist_credit_name(artist_id, &name).await?)
    }

    /// Rename one of an artist's other printed names; every credit printing
    /// it prints the new name.
    pub async fn rename_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
        name: &str,
    ) -> Result<(), LibraryError> {
        let name = credit_name(name)?;
        Ok(self.database.rename_artist_credit_name(artist_id, key, &name).await?)
    }

    /// Remove one of an artist's other printed names; every credit printing
    /// it prints the artist's own name.
    pub async fn remove_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
    ) -> Result<(), LibraryError> {
        Ok(self.database.remove_artist_credit_name(artist_id, key).await?)
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

/// A credit name a person entered, refused when blank.
fn credit_name(name: &str) -> Result<bae_store::EnteredName, LibraryError> {
    bae_store::EnteredName::parse(name).map_err(|bae_store::BlankName| {
        LibraryError::Refused("a credit name cannot be blank".into())
    })
}

#[cfg(test)]
#[path = "artist_tests.rs"]
mod tests;
