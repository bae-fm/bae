use super::*;
use bae_people::collections::{CollectionError, RatingTarget, Stars};
use bae_people::people::Sharing;

impl AppServices {
    delegate_async!(people, create_profile => create_profile(name: &str) -> Result<(), bae_people::people::ProfileError>);
    delegate_async!(people, rename_profile => rename_profile(name: &str) -> Result<(), bae_people::people::ProfileError>);
    delegate_async!(people, set_history_sharing => set_history_sharing(sharing: bae_people::people::HistorySharing) -> Result<(), bae_people::people::ProfileError>);
    delegate_async!(manager, set_avatar => set_avatar(bytes: Option<Vec<u8>>) -> Result<(), bae_library::people::AvatarError>);
    delegate_async!(people, avatar => avatar(person: coven::MemberId) -> Result<Option<Vec<u8>>, bae_people::people::ProfileError>);
    delegate_values!(subscribe_profiles => subscribe_profiles() -> Vec<bae_people::people::PersonProfile>);
    /// Every playlist this device holds, now and after each change.
    pub fn subscribe_playlists(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_views::playlists::PlaylistListing> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_playlists())
    }
    delegate_values!(subscribe_requested_profile => subscribe_requested_profile() -> Option<bae_people::people::RequestedProfile>);
    delegate_lib!(people, people => people() -> Vec<bae_people::people::Person>);
    delegate_lib!(people, own_person => own_person() -> Option<bae_people::people::Person>);
    delegate_async!(people, set_rating => set_rating(target: RatingTarget, stars: Option<Stars>) -> Result<(), CollectionError>);
    delegate_async!(manager, create_playlist => create_playlist(name: &str, sharing: Sharing, tracks: Vec<String>) -> Result<uuid::Uuid, CollectionError>);
    delegate_async!(people, rename_playlist => rename_playlist(id: uuid::Uuid, name: &str) -> Result<(), CollectionError>);
    delegate_async!(people, set_playlist_sharing => set_playlist_sharing(id: uuid::Uuid, sharing: Sharing) -> Result<(), CollectionError>);
    delegate_async!(people, delete_playlist => delete_playlist(id: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(manager, append_playlist_entry => append_playlist_entry(playlist: uuid::Uuid, track: String) -> Result<uuid::Uuid, CollectionError>);
    delegate_async!(people, remove_playlist_entry => remove_playlist_entry(playlist: uuid::Uuid, position: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(people, reorder_playlist => reorder_playlist(playlist: uuid::Uuid, order: Vec<uuid::Uuid>) -> Result<(), CollectionError>);
}

/// Why a playlist play has no outcome to tell.
#[derive(Debug, thiserror::Error)]
pub enum PlayPlaylistError {
    #[error("couldn't read the playlist: {0}")]
    Entries(#[source] bae_store::DbError),
    #[error(transparent)]
    Play(#[from] bae_playback::PlayError),
}

impl AppServices {
    /// Play a playlist's tracks as a list in the playlist's order, from the
    /// entry `start` names or from the first. An entry the playlist no longer
    /// holds as `start` plays nothing and answers start missing; a playlist
    /// without entries answers nothing to play.
    pub async fn play_playlist(
        &self,
        id: uuid::Uuid,
        start: Option<uuid::Uuid>,
    ) -> Result<bae_playback::PlayOutcome, PlayPlaylistError> {
        let entries =
            self.inner.people.playlist_entries(id).await.map_err(PlayPlaylistError::Entries)?;
        let start_index = match start {
            None => 0,
            Some(position) => {
                match entries.iter().position(|entry| entry.position_id == position) {
                    Some(index) => index,
                    None => return Ok(bae_playback::PlayOutcome::StartMissing),
                }
            }
        };
        let track_ids = entries.into_iter().map(|entry| entry.track_id).collect();
        Ok(self
            .playback_play_tracks(track_ids, bae_playback::tables::TracksStart::Index(start_index))
            .await?)
    }
}
