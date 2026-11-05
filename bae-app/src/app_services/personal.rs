use super::*;
use bae_db::collections::{CollectionError, Playlist, PlaylistEntry, Rating, RatingTarget, Stars};
use bae_db::people::Sharing;
use bae_library::LibraryError;

impl AppServices {
    delegate_async!(manager, create_profile => create_profile(name: &str) -> Result<(), bae_db::people::ProfileError>);
    delegate_async!(manager, rename_profile => rename_profile(name: &str) -> Result<(), bae_db::people::ProfileError>);
    delegate_async!(manager, set_history_sharing => set_history_sharing(sharing: bae_db::people::Sharing) -> Result<(), bae_db::people::ProfileError>);
    delegate_async!(manager, set_avatar => set_avatar(bytes: Option<Vec<u8>>) -> Result<(), bae_library::people::AvatarError>);
    delegate_async!(manager, avatar => avatar(person: coven::MemberId) -> Result<Option<Vec<u8>>, bae_db::people::ProfileError>);
    delegate_values!(subscribe_profiles => subscribe_profiles() -> Vec<bae_db::people::PersonProfile>);
    delegate_values!(subscribe_requested_profile => subscribe_requested_profile() -> Option<bae_db::people::RequestedProfile>);
    delegate_async!(manager, people => people() -> Result<Vec<bae_db::people::Person>, LibraryError>);
    delegate_async!(manager, own_person => own_person() -> Result<Option<bae_db::people::Person>, LibraryError>);
    delegate_async!(manager, set_rating => set_rating(target: RatingTarget, stars: Option<Stars>) -> Result<(), CollectionError>);
    delegate_async!(manager, ratings => ratings() -> Result<Vec<Rating>, bae_store::DbError>);
    delegate_async!(manager, create_playlist => create_playlist(name: &str, sharing: Sharing) -> Result<Playlist, CollectionError>);
    delegate_async!(manager, playlists => playlists() -> Result<Vec<Playlist>, bae_store::DbError>);
    delegate_async!(manager, rename_playlist => rename_playlist(id: uuid::Uuid, name: &str) -> Result<(), CollectionError>);
    delegate_async!(manager, set_playlist_sharing => set_playlist_sharing(id: uuid::Uuid, sharing: Sharing) -> Result<(), CollectionError>);
    delegate_async!(manager, delete_playlist => delete_playlist(id: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(manager, playlist_entries => playlist_entries(id: uuid::Uuid) -> Result<Vec<PlaylistEntry>, bae_store::DbError>);
    delegate_async!(manager, append_playlist_entry => append_playlist_entry(playlist: uuid::Uuid, track: String) -> Result<uuid::Uuid, CollectionError>);
    delegate_async!(manager, remove_playlist_entry => remove_playlist_entry(playlist: uuid::Uuid, position: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(manager, reorder_playlist => reorder_playlist(playlist: uuid::Uuid, order: Vec<uuid::Uuid>) -> Result<(), CollectionError>);
}
