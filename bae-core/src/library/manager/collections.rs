use super::*;
use crate::collections::{CollectionError, Playlist, PlaylistEntry, Rating, RatingTarget, Stars};
use crate::people::Sharing;

impl LibraryManager {
    pub async fn set_rating(
        &self,
        target: RatingTarget,
        stars: Option<Stars>,
    ) -> Result<(), CollectionError> {
        self.database.set_rating(target, stars).await
    }

    pub async fn ratings(&self) -> Result<Vec<Rating>, crate::db::DbError> {
        self.database.ratings().await
    }

    pub fn subscribe_ratings(&self) -> coven::LiveQuery<Vec<Rating>> {
        self.database.subscribe_ratings()
    }

    pub async fn create_playlist(
        &self,
        name: &str,
        sharing: Sharing,
    ) -> Result<Playlist, CollectionError> {
        self.database
            .create_playlist(self.ids.new_id(), name, sharing)
            .await
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>, crate::db::DbError> {
        self.database.playlists().await
    }

    pub fn subscribe_playlists(&self) -> coven::LiveQuery<Vec<Playlist>> {
        self.database.subscribe_playlists()
    }

    pub async fn rename_playlist(&self, id: uuid::Uuid, name: &str) -> Result<(), CollectionError> {
        self.database.rename_playlist(id, name).await
    }

    pub async fn set_playlist_sharing(
        &self,
        id: uuid::Uuid,
        sharing: Sharing,
    ) -> Result<(), CollectionError> {
        self.database.set_playlist_sharing(id, sharing).await
    }

    pub async fn delete_playlist(&self, id: uuid::Uuid) -> Result<(), CollectionError> {
        self.database.delete_playlist(id).await
    }

    pub async fn playlist_entries(
        &self,
        id: uuid::Uuid,
    ) -> Result<Vec<PlaylistEntry>, crate::db::DbError> {
        self.database.playlist_entries(id).await
    }

    pub fn subscribe_playlist_entries(
        &self,
        id: uuid::Uuid,
    ) -> coven::LiveQuery<Vec<PlaylistEntry>> {
        self.database.subscribe_playlist_entries(id)
    }

    pub async fn append_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        track: String,
    ) -> Result<uuid::Uuid, CollectionError> {
        let position = self.ids.new_id();
        self.database
            .append_playlist_entry(playlist, position, track)
            .await?;
        Ok(position)
    }

    pub async fn remove_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        position: uuid::Uuid,
    ) -> Result<(), CollectionError> {
        self.database
            .remove_playlist_entry(playlist, position)
            .await
    }

    pub async fn reorder_playlist(
        &self,
        playlist: uuid::Uuid,
        order: Vec<uuid::Uuid>,
    ) -> Result<(), CollectionError> {
        self.database.reorder_playlist(playlist, order).await
    }
}
