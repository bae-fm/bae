use super::*;
use bae_people::collections::{CollectionError, Playlist};
use bae_people::people::Sharing;

impl LibraryManager {
    pub async fn create_playlist(
        &self,
        name: &str,
        sharing: Sharing,
    ) -> Result<Playlist, CollectionError> {
        self.people.create_playlist(self.ids.new_id(), name, sharing).await
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
