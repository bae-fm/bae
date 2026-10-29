use super::*;
use crate::collections::{CollectionError, Playlist, PlaylistEntry, Rating, RatingTarget, Stars};
use crate::library::LibraryError;
use crate::people::Sharing;

impl AppServices {
    delegate_async!(manager, create_profile => create_profile(name: &str) -> Result<crate::people::PersonProfile, crate::people::ProfileError>);
    delegate_async!(manager, rename_profile => rename_profile(name: &str) -> Result<(), crate::people::ProfileError>);
    delegate_async!(manager, set_history_sharing => set_history_sharing(sharing: crate::people::Sharing) -> Result<(), crate::people::ProfileError>);
    delegate_async!(manager, set_avatar => set_avatar(bytes: Option<Vec<u8>>) -> Result<(), crate::people::ProfileError>);
    delegate_async!(manager, avatar => avatar(person: coven::MemberId) -> Result<Option<Vec<u8>>, crate::people::ProfileError>);
    delegate_sync!(manager, subscribe_profiles => subscribe_profiles() -> coven::LiveQuery<Vec<crate::people::PersonProfile>>);
    delegate_async!(manager, people => people() -> Result<Vec<crate::people::Person>, LibraryError>);
    delegate_async!(manager, own_person => own_person() -> Result<Option<crate::people::Person>, LibraryError>);
    delegate_async!(manager, set_rating => set_rating(target: RatingTarget, stars: Option<Stars>) -> Result<(), CollectionError>);
    delegate_async!(manager, ratings => ratings() -> Result<Vec<Rating>, crate::db::DbError>);
    delegate_sync!(manager, subscribe_ratings => subscribe_ratings() -> coven::LiveQuery<Vec<Rating>>);
    delegate_async!(manager, create_playlist => create_playlist(name: &str, sharing: Sharing) -> Result<Playlist, CollectionError>);
    delegate_async!(manager, playlists => playlists() -> Result<Vec<Playlist>, crate::db::DbError>);
    delegate_sync!(manager, subscribe_playlists => subscribe_playlists() -> coven::LiveQuery<Vec<Playlist>>);
    delegate_async!(manager, rename_playlist => rename_playlist(id: uuid::Uuid, name: &str) -> Result<(), CollectionError>);
    delegate_async!(manager, set_playlist_sharing => set_playlist_sharing(id: uuid::Uuid, sharing: Sharing) -> Result<(), CollectionError>);
    delegate_async!(manager, delete_playlist => delete_playlist(id: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(manager, playlist_entries => playlist_entries(id: uuid::Uuid) -> Result<Vec<PlaylistEntry>, crate::db::DbError>);
    delegate_sync!(manager, subscribe_playlist_entries => subscribe_playlist_entries(id: uuid::Uuid) -> coven::LiveQuery<Vec<PlaylistEntry>>);
    delegate_async!(manager, append_playlist_entry => append_playlist_entry(playlist: uuid::Uuid, track: String) -> Result<uuid::Uuid, CollectionError>);
    delegate_async!(manager, remove_playlist_entry => remove_playlist_entry(playlist: uuid::Uuid, position: uuid::Uuid) -> Result<(), CollectionError>);
    delegate_async!(manager, reorder_playlist => reorder_playlist(playlist: uuid::Uuid, order: Vec<uuid::Uuid>) -> Result<(), CollectionError>);
}
