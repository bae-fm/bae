use super::*;
use bae_people::collections::CollectionError;
use bae_people::people::Sharing;

impl LibraryManager {
    /// Makes a playlist holding `tracks` in the list's order, from its first
    /// write, and returns its id.
    pub async fn create_playlist(
        &self,
        name: &str,
        sharing: Sharing,
        tracks: impl Into<bae_records::TrackList>,
    ) -> Result<uuid::Uuid, CollectionError> {
        let tracks = tracks.into();
        let positions = (0..tracks.track_count()).map(|_| self.ids.new_id()).collect();
        let playlist = self
            .people
            .create_playlist(self.ids.new_id(), name, sharing, positions, tracks)
            .await?;
        Ok(playlist.id)
    }

    pub async fn append_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        track: String,
    ) -> Result<uuid::Uuid, CollectionError> {
        let position = self.ids.new_id();
        self.people.append_playlist_entry(playlist, position, track).await?;
        Ok(position)
    }

    /// Adds `tracks` to the end of the playlist in the list's order, in one
    /// write.
    pub async fn append_playlist_entries(
        &self,
        playlist: uuid::Uuid,
        tracks: impl Into<bae_records::TrackList>,
    ) -> Result<(), CollectionError> {
        let tracks = tracks.into();
        let positions = (0..tracks.track_count()).map(|_| self.ids.new_id()).collect();
        self.people.append_playlist_entries(playlist, positions, tracks).await
    }

    /// Adds `tracks` at `place` in the playlist in the list's order, in one
    /// write.
    pub async fn insert_playlist_entries(
        &self,
        playlist: uuid::Uuid,
        tracks: impl Into<bae_records::TrackList>,
        place: bae_people::collections::BesideEntry,
    ) -> Result<(), CollectionError> {
        let tracks = tracks.into();
        let positions = (0..tracks.track_count()).map(|_| self.ids.new_id()).collect();
        self.people.insert_playlist_entries(playlist, positions, tracks, place).await
    }
}
