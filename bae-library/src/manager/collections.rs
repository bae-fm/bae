use super::*;
use bae_people::collections::CollectionError;
use bae_people::people::Sharing;

impl LibraryManager {
    /// Makes a playlist holding `tracks` in that order, from its first write,
    /// and returns its id.
    pub async fn create_playlist(
        &self,
        name: &str,
        sharing: Sharing,
        tracks: Vec<String>,
    ) -> Result<uuid::Uuid, CollectionError> {
        let entries = tracks.into_iter().map(|track| (self.ids.new_id(), track)).collect();
        let playlist =
            self.people.create_playlist(self.ids.new_id(), name, sharing, entries).await?;
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
}
