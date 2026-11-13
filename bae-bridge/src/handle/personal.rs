use super::*;
use crate::types::*;

forward! { async this => {
    fn create_profile(name: String) -> () { Ok(this.services.create_profile(&name).await?) }
    fn rename_profile(name: String) -> () { Ok(this.services.rename_profile(&name).await?) }
    fn set_history_sharing(sharing: BridgeHistorySharing) -> () { Ok(this.services.set_history_sharing(sharing.into_core()).await?) }
    fn set_avatar(bytes: Option<Vec<u8>>) -> () { Ok(this.services.set_avatar(bytes).await?) }
    fn avatar(person: String) -> Option<Vec<u8>> { Ok(this.services.avatar(crate::from_app::member(&person)?).await?) }
    fn set_rating(target: BridgeRatingTarget, stars: Option<BridgeStars>) -> () { Ok(this.services.set_rating(target.into_core(), stars.map(BridgeStars::into_core)).await?) }
    fn ratings() -> Vec<BridgeRating> { Ok(this.services.ratings().await?.into_iter().map(BridgeRating::from_core).collect()) }
    fn create_playlist(name: String, sharing: BridgeSharing, track_ids: Vec<String>) -> String { Ok(this.services.create_playlist(&name,sharing.into_core(),track_ids).await?.to_string()) }
    fn play_playlist(playlist_id: String, start_position_id: Option<String>) -> BridgePlayOutcome {
        let start = start_position_id.map(|id| id.parse().map_err(BridgeError::config)).transpose()?;
        Ok(BridgePlayOutcome::from_core(this.services.play_playlist(playlist_id.parse().map_err(BridgeError::config)?, start).await?))
    }
    fn rename_playlist(id: String, name: String) -> () { Ok(this.services.rename_playlist(id.parse().map_err(BridgeError::config)?,&name).await?) }
    fn set_playlist_sharing(id: String, sharing: BridgeSharing) -> () { Ok(this.services.set_playlist_sharing(id.parse().map_err(BridgeError::config)?,sharing.into_core()).await?) }
    fn delete_playlist(id: String) -> () { Ok(this.services.delete_playlist(id.parse().map_err(BridgeError::config)?).await?) }
    fn append_playlist_entry(playlist: String, track: String) -> String { Ok(this.services.append_playlist_entry(playlist.parse().map_err(BridgeError::config)?,track).await?.to_string()) }
    fn remove_playlist_entry(playlist: String, position: String) -> () { Ok(this.services.remove_playlist_entry(playlist.parse().map_err(BridgeError::config)?,position.parse().map_err(BridgeError::config)?).await?) }
    fn reorder_playlist(playlist: String, order: Vec<String>) -> () { Ok(this.services.reorder_playlist(playlist.parse().map_err(BridgeError::config)?,order.into_iter().map(|id| id.parse().map_err(BridgeError::config)).collect::<Result<_,_>>()?).await?) }
    fn library_name() -> String { Ok(this.services.library_name().await?) }
} }

/// Whether a name typed for a library, profile or playlist is blank, which
/// core refuses.
#[uniffi::export]
pub fn entered_name_is_blank(text: String) -> bool {
    bae_store::EnteredName::parse(&text).is_err()
}

#[uniffi::export]
impl AppHandle {
    /// Every person's profile, now and after each change.
    pub fn subscribe_profiles(&self) -> std::sync::Arc<crate::ProfilesSubscription> {
        crate::screens::profiles(self.services.subscribe_profiles(), self.runtime.clone())
    }

    /// Every playlist this device holds, now and after each change.
    pub fn subscribe_playlists(&self) -> std::sync::Arc<crate::PlaylistsSubscription> {
        crate::screens::playlists(self.services.subscribe_playlists(), self.runtime.clone())
    }

    /// The profile this person asked for, until it is written, now and
    /// after each change.
    pub fn subscribe_requested_profile(
        &self,
    ) -> std::sync::Arc<crate::RequestedProfileSubscription> {
        crate::screens::requested_profile(
            self.services.subscribe_requested_profile(),
            self.runtime.clone(),
        )
    }

    /// The library's name, now and after each rename.
    pub fn subscribe_library_name(&self) -> std::sync::Arc<crate::LibraryNameSubscription> {
        crate::screens::library_name(self.services.subscribe_library_name(), self.runtime.clone())
    }
}

#[cfg(test)]
#[path = "personal_tests.rs"]
mod tests;
