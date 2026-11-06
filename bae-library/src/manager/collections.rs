use super::*;
use bae_people::collections::{
    CollectionError, Playlist, PlaylistEntry, Rating, RatingTarget, Stars,
};
use bae_people::people::Sharing;

impl LibraryManager {
    pub async fn set_rating(
        &self,
        target: RatingTarget,
        stars: Option<Stars>,
    ) -> Result<(), CollectionError> {
        self.people.set_rating(target, stars).await
    }

    pub async fn ratings(&self) -> Result<Vec<Rating>, bae_store::DbError> {
        self.people.ratings().await
    }

    pub async fn create_playlist(
        &self,
        name: &str,
        sharing: Sharing,
    ) -> Result<Playlist, CollectionError> {
        self.people.create_playlist(self.ids.new_id(), name, sharing).await
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>, bae_store::DbError> {
        self.people.playlists().await
    }

    pub async fn rename_playlist(&self, id: uuid::Uuid, name: &str) -> Result<(), CollectionError> {
        self.people.rename_playlist(id, name).await
    }

    pub async fn set_playlist_sharing(
        &self,
        id: uuid::Uuid,
        sharing: Sharing,
    ) -> Result<(), CollectionError> {
        self.people.set_playlist_sharing(id, sharing).await
    }

    pub async fn delete_playlist(&self, id: uuid::Uuid) -> Result<(), CollectionError> {
        self.people.delete_playlist(id).await
    }

    pub async fn playlist_entries(
        &self,
        id: uuid::Uuid,
    ) -> Result<Vec<PlaylistEntry>, bae_store::DbError> {
        self.people.playlist_entries(id).await
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

    pub async fn remove_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        position: uuid::Uuid,
    ) -> Result<(), CollectionError> {
        self.people.remove_playlist_entry(playlist, position).await
    }

    pub async fn reorder_playlist(
        &self,
        playlist: uuid::Uuid,
        order: Vec<uuid::Uuid>,
    ) -> Result<(), CollectionError> {
        self.people.reorder_playlist(playlist, order).await
    }
}
