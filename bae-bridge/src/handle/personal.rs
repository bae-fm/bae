use super::*;
use crate::types::*;

forward! { async this => {
    fn create_profile(name: String) -> BridgePersonProfile { Ok(BridgePersonProfile::from_core(this.services.create_profile(&name).await?)) }
    fn rename_profile(name: String) -> () { Ok(this.services.rename_profile(&name).await?) }
    fn set_history_sharing(sharing: BridgeSharing) -> () { Ok(this.services.set_history_sharing(sharing.into_core()).await?) }
    fn set_avatar(bytes: Option<Vec<u8>>) -> () { Ok(this.services.set_avatar(bytes).await?) }
    fn avatar(person: String) -> Option<Vec<u8>> { Ok(this.services.avatar(coven::MemberId(person.parse().map_err(BridgeError::config)?)).await?) }
    fn set_rating(target: BridgeRatingTarget, stars: Option<BridgeStars>) -> () { Ok(this.services.set_rating(target.into_core(), stars.map(BridgeStars::into_core)).await?) }
    fn ratings() -> Vec<BridgeRating> { Ok(this.services.ratings().await?.into_iter().map(BridgeRating::from_core).collect()) }
    fn create_playlist(name: String, sharing: BridgeSharing) -> BridgePlaylist { Ok(BridgePlaylist::from_core(this.services.create_playlist(&name,sharing.into_core()).await?)) }
    fn playlists() -> Vec<BridgePlaylist> { Ok(this.services.playlists().await?.into_iter().map(BridgePlaylist::from_core).collect()) }
    fn rename_playlist(id: String, name: String) -> () { Ok(this.services.rename_playlist(id.parse().map_err(BridgeError::config)?,&name).await?) }
    fn set_playlist_sharing(id: String, sharing: BridgeSharing) -> () { Ok(this.services.set_playlist_sharing(id.parse().map_err(BridgeError::config)?,sharing.into_core()).await?) }
    fn delete_playlist(id: String) -> () { Ok(this.services.delete_playlist(id.parse().map_err(BridgeError::config)?).await?) }
    fn playlist_entries(id: String) -> Vec<BridgePlaylistEntry> { Ok(this.services.playlist_entries(id.parse().map_err(BridgeError::config)?).await?.into_iter().map(BridgePlaylistEntry::from_core).collect()) }
    fn append_playlist_entry(playlist: String, track: String) -> String { Ok(this.services.append_playlist_entry(playlist.parse().map_err(BridgeError::config)?,track).await?.to_string()) }
    fn remove_playlist_entry(playlist: String, position: String) -> () { Ok(this.services.remove_playlist_entry(playlist.parse().map_err(BridgeError::config)?,position.parse().map_err(BridgeError::config)?).await?) }
    fn reorder_playlist(playlist: String, order: Vec<String>) -> () { Ok(this.services.reorder_playlist(playlist.parse().map_err(BridgeError::config)?,order.into_iter().map(|id| id.parse().map_err(BridgeError::config)).collect::<Result<_,_>>()?).await?) }
    fn library_name() -> String { Ok(this.services.library_name().await?) }
} }

#[uniffi::export(callback_interface)]
pub trait ProfilesCallback: Send + Sync {
    fn on_value(&self, value: Vec<BridgePersonProfile>);
    fn on_error(&self, error: BridgeError);
}
#[uniffi::export]
impl AppHandle {
    pub fn subscribe_profiles(
        &self,
        callback: Box<dyn ProfilesCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_query(
            move |services| services.subscribe_profiles(),
            move |result| match result {
                Ok(value) => callback.on_value(
                    value
                        .into_iter()
                        .map(BridgePersonProfile::from_core)
                        .collect(),
                ),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
}
#[uniffi::export(callback_interface)]
pub trait RatingsCallback: Send + Sync {
    fn on_value(&self, value: Vec<BridgeRating>);
    fn on_error(&self, error: BridgeError);
}
#[uniffi::export]
impl AppHandle {
    pub fn subscribe_ratings(
        &self,
        callback: Box<dyn RatingsCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_query(
            move |services| services.subscribe_ratings(),
            move |result| match result {
                Ok(value) => {
                    callback.on_value(value.into_iter().map(BridgeRating::from_core).collect())
                }
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
}
#[uniffi::export(callback_interface)]
pub trait PlaylistsCallback: Send + Sync {
    fn on_value(&self, value: Vec<BridgePlaylist>);
    fn on_error(&self, error: BridgeError);
}
#[uniffi::export]
impl AppHandle {
    pub fn subscribe_playlists(
        &self,
        callback: Box<dyn PlaylistsCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_query(
            move |services| services.subscribe_playlists(),
            move |result| match result {
                Ok(value) => {
                    callback.on_value(value.into_iter().map(BridgePlaylist::from_core).collect())
                }
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
}
#[uniffi::export(callback_interface)]
pub trait PlaylistEntriesCallback: Send + Sync {
    fn on_value(&self, value: Vec<BridgePlaylistEntry>);
    fn on_error(&self, error: BridgeError);
}
#[uniffi::export]
impl AppHandle {
    pub fn subscribe_playlist_entries(
        &self,
        id: String,
        callback: Box<dyn PlaylistEntriesCallback>,
    ) -> Result<std::sync::Arc<crate::LiveSubscription>, BridgeError> {
        let id = id.parse().map_err(BridgeError::config)?;
        Ok(self.subscribe_query(
            move |services| services.subscribe_playlist_entries(id),
            move |result| match result {
                Ok(value) => callback.on_value(
                    value
                        .into_iter()
                        .map(BridgePlaylistEntry::from_core)
                        .collect(),
                ),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        ))
    }
}
#[uniffi::export(callback_interface)]
pub trait LibraryNameCallback: Send + Sync {
    fn on_value(&self, value: String);
    fn on_error(&self, error: BridgeError);
}
#[uniffi::export]
impl AppHandle {
    pub fn subscribe_library_name(
        &self,
        callback: Box<dyn LibraryNameCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_query(
            move |services| services.subscribe_library_name(),
            move |result| match result {
                Ok(value) => callback.on_value(value),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
}

#[cfg(test)]
#[path = "personal_tests.rs"]
mod tests;
