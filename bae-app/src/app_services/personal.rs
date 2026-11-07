use super::*;
use bae_people::collections::{
    CollectionError, Playlist, PlaylistEntry, Rating, RatingTarget, Stars,
};
use bae_people::people::Sharing;

impl AppServices {
    delegate_async!(people, create_profile => create_profile(name: &str) -> Result<(), bae_people::people::ProfileError>);
    delegate_async!(people, rename_profile => rename_profile(name: &str) -> Result<(), bae_people::people::ProfileError>);
    delegate_async!(people, set_history_sharing => set_history_sharing(sharing: bae_people::people::Sharing) -> Result<(), bae_people::people::ProfileError>);
    delegate_async!(manager, set_avatar => set_avatar(bytes: Option<Vec<u8>>) -> Result<(), bae_library::people::AvatarError>);
    delegate_async!(people, avatar => avatar(person: coven::MemberId) -> Result<Option<Vec<u8>>, bae_people::people::ProfileError>);
    delegate_values!(subscribe_profiles => subscribe_profiles() -> Vec<bae_people::people::PersonProfile>);
    delegate_values!(subscribe_requested_profile => subscribe_requested_profile() -> Option<bae_people::people::RequestedProfile>);
    delegate_lib!(people, people => people() -> Vec<bae_people::people::Person>);
    delegate_lib!(people, own_person => own_person() -> Option<bae_people::people::Person>);
    delegate_async!(people, set_rating => set_rating(target: RatingTarget, stars: Option<Stars>) -> Result<(), CollectionError>);
    delegate_async!(people, ratings => ratings() -> Result<Vec<Rating>, bae_store::DbError>);
    delegate_async!(manager, create_playlist => create_playlist(name: &str, sharing: Sharing) -> Result<Playlist, CollectionError>);
    delegate_async!(people, playlists => playlists() -> Result<Vec<Playlist>, bae_store::DbError>);
    delegate_async!(people, rename_playlist => rename_playlist(id: uuid::Uuid, name: &str) -> Result<(), CollectionError>);
    delegate_async!(people, set_playlist_sharing => set_playlist_sharing(id: uuid::Uuid, sharing: Sharing) -> Result<(), CollectionError>);
    delegate_async!(people, delete_playlist => delete_playlist(id: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(people, playlist_entries => playlist_entries(id: uuid::Uuid) -> Result<Vec<PlaylistEntry>, bae_store::DbError>);
    delegate_async!(manager, append_playlist_entry => append_playlist_entry(playlist: uuid::Uuid, track: String) -> Result<uuid::Uuid, CollectionError>);
    delegate_async!(people, remove_playlist_entry => remove_playlist_entry(playlist: uuid::Uuid, position: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(people, reorder_playlist => reorder_playlist(playlist: uuid::Uuid, order: Vec<uuid::Uuid>) -> Result<(), CollectionError>);
}
