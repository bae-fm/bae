//! Membership belongs to people; each person can register several devices.

use super::*;

/// How often this device syncs while Add Device's sheet watches for the new
/// device. A quiet pass is two requests: about 24 a minute, only while the
/// sheet is open.
const ADD_DEVICE_SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

impl LibraryManager {
    /// Create this person's profile: at once when their private circle
    /// exists, otherwise once sync has made it (§5.4).
    pub async fn create_profile(&self, name: &str) -> Result<(), bae_people::people::ProfileError> {
        self.people.create_profile(name).await
    }

    pub async fn rename_profile(&self, name: &str) -> Result<(), bae_people::people::ProfileError> {
        self.people.rename_profile(name).await
    }

    pub async fn set_history_sharing(
        &self,
        sharing: bae_people::people::Sharing,
    ) -> Result<(), bae_people::people::ProfileError> {
        self.people.set_history_sharing(sharing).await
    }

    /// Set this person's avatar to the image in `bytes`, downscaled to the
    /// JPEG bae stores for covers and artist pictures, or clear it with `None`.
    pub async fn set_avatar(
        &self,
        bytes: Option<Vec<u8>>,
    ) -> Result<(), crate::people::AvatarError> {
        let avatar = match bytes {
            Some(bytes) => {
                // Decoding and re-encoding the image is CPU work.
                let jpeg = self
                    .diagnostics
                    .blocking("resize an avatar", move || {
                        bae_util::stored_image::stored_jpeg(&bytes)
                    })
                    .await?
                    .map_err(|detail| crate::people::AvatarError::Image { detail })?;
                Some(jpeg)
            }
            None => None,
        };
        Ok(self.people.set_avatar(avatar).await?)
    }

    pub async fn avatar(
        &self,
        person: coven::MemberId,
    ) -> Result<Option<Vec<u8>>, bae_people::people::ProfileError> {
        self.people.avatar(person).await
    }

    /// The profile this person asked for, until it is written, live.
    pub fn subscribe_requested_profile(
        &self,
    ) -> coven::LiveQuery<Option<bae_people::people::RequestedProfile>, bae_store::DbError> {
        self.people.subscribe_requested_profile()
    }

    pub fn subscribe_profiles(
        &self,
    ) -> coven::LiveQuery<Vec<bae_people::people::PersonProfile>, bae_store::DbError> {
        self.people.subscribe_profiles()
    }

    pub async fn people(&self) -> Result<Vec<bae_people::people::Person>, LibraryError> {
        Ok(self.people.people().await?)
    }

    /// The person using this device, once their registration has applied.
    pub async fn own_person(&self) -> Result<Option<bae_people::people::Person>, LibraryError> {
        Ok(self.people.own_person().await?)
    }

    /// This person's restore code, and for the QR code the one that holds, on
    /// S3, the key this device holds, for their own new device.
    pub async fn generate_restore_code(
        &self,
    ) -> Result<crate::sync::restore_code::RestoreCodeShare, LibraryError> {
        let code = self.store.generate_restore_code().await?;
        let qr_text = self.store.restore_code_for_transfer().await?;
        crate::sync::restore_code::RestoreCodeShare::new(code, qr_text)
            .map_err(LibraryError::TransferCode)
    }

    pub async fn get_members(&self) -> Result<bae_people::membership::Members, LibraryError> {
        Ok(self.membership.members().await?)
    }

    pub fn subscribe_members(
        &self,
    ) -> coven::LiveQuery<bae_people::membership::Members, bae_store::DbError> {
        self.membership.subscribe_members()
    }

    /// Add Device's sheet: this person's restore code to show, and the watch
    /// for their device that joins from it. The person's devices are read
    /// before the code is made, so a device restored from this code is never
    /// among them.
    pub async fn add_device(&self) -> Result<crate::sync::membership::AddingDevice, LibraryError> {
        let watch = crate::sync::membership::AddedDeviceWatch::new(&self.get_members().await?);
        let share = self.generate_restore_code().await?;
        Ok(crate::sync::membership::AddingDevice { share, watch })
    }

    /// This person's device that `watch` finds: none until one joins. Add
    /// Device's sheet subscribes while it shows the code. While the
    /// subscription lives, this device syncs every 5 seconds instead of every
    /// 30, so the sheet hears of the new device soon after it registers.
    pub fn subscribe_added_device(
        &self,
        watch: crate::sync::membership::AddedDeviceWatch,
    ) -> coven::LiveQuery<Option<bae_people::membership::ActiveDevice>, bae_store::DbError> {
        let sync_often = self.store.sync_often(ADD_DEVICE_SYNC_INTERVAL);
        self.subscribe_members().process(move |membership| {
            // The subscription owns the request; dropping it ends the request.
            let _sync_often = &sync_often;
            Ok(watch.added(&membership))
        })
    }

    /// The admin changes this device asked for, each until the person
    /// dismisses its outcome, live (§5.6).
    pub fn subscribe_admin_changes(
        &self,
    ) -> coven::LiveQuery<Vec<bae_people::admin_change::AdminChange>, bae_store::DbError> {
        self.membership.subscribe_admin_changes()
    }

    /// Dismiss a finished admin change: coven's record of its outcome and
    /// this device's row about it go together.
    pub async fn acknowledge_admin_change(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), LibraryError> {
        Ok(self.membership.acknowledge_admin_change(operation).await?)
    }

    /// Remove a member. The change is recorded on this device at once,
    /// offline too, and takes effect once sync publishes it (§5.6).
    pub async fn remove_member(&self, member: &coven::MemberId) -> Result<(), LibraryError> {
        Ok(self.membership.remove_member(member).await?)
    }

    /// Remove a device, recorded as a removal is, and say what is left to
    /// do at the provider.
    pub async fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<Option<coven::ProviderSignOut>, LibraryError> {
        Ok(self.membership.remove_device(device).await?)
    }

    /// Change a member's role, recorded as a removal is.
    pub async fn set_member_role(
        &self,
        member: &coven::MemberId,
        role: coven::MemberRole,
    ) -> Result<(), LibraryError> {
        Ok(self.membership.set_member_role(member, role).await?)
    }

    pub async fn approve_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), LibraryError> {
        Ok(self.membership.approve_join_request(request).await?)
    }

    /// Cancel an invite and take back its access. Once approval of its
    /// person has begun, the invite can't be cancelled (§5.6).
    pub async fn cancel_invite(&self, invite: &coven::InviteId) -> Result<(), LibraryError> {
        match self.store.cancel_invite(invite).await {
            Err(error @ coven::SyncError::InvitationChanged) => {
                Err(LibraryError::InviteJoining(error))
            }
            result => Ok(result?),
        }
    }
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;
