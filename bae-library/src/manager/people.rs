//! Membership belongs to people; each person can register several devices.

use super::*;

impl LibraryManager {
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
        let watch =
            crate::sync::membership::AddedDeviceWatch::new(&self.membership.members().await?);
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
        let sync_often = self.store.sync_often(crate::bootstrap::WAITING_SCREEN_CHECK_INTERVAL);
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

    /// Whether this device's person's account holds the library's storage:
    /// only then can they confirm a device cut off (coven §20.9).
    pub async fn holds_store(&self) -> Result<bool, LibraryError> {
        Ok(self.store.holds_store().await?)
    }

    /// The person did the provider step for a device they removed, so
    /// nothing it sent can land any more (coven §14.2). While a provider
    /// request to one of the accounts it shared runs, nothing changes and
    /// the person confirms again once it ends.
    pub async fn confirm_device_cut_off(
        &self,
        device: coven::DeviceId,
    ) -> Result<DeviceCutOff, LibraryError> {
        device_cut_off(self.store.confirm_device_cut_off(device).await)
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

/// How confirming a device cut off ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceCutOff {
    /// This device takes back the accounts the removed device's shares named.
    Confirmed,
    /// A provider request to one of those accounts runs; nothing changed.
    ProviderRequestRunning,
    /// The device's removal hasn't reached this device's history yet, or it
    /// was never removed; nothing changed, and after the next sync it may work.
    DeviceNotGone,
}

/// A request running, and a device the history doesn't have gone yet, are the
/// refusals that ask the person to try again; the others can't succeed.
pub(super) fn device_cut_off(
    result: Result<(), coven::SyncError>,
) -> Result<DeviceCutOff, LibraryError> {
    match result {
        Ok(()) => Ok(DeviceCutOff::Confirmed),
        Err(coven::SyncError::Database(coven::DbError::ProviderRequestRunning)) => {
            Ok(DeviceCutOff::ProviderRequestRunning)
        }
        Err(coven::SyncError::CutOffRefused(coven::CutOffRefusal::DeviceNotGone)) => {
            Ok(DeviceCutOff::DeviceNotGone)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;
