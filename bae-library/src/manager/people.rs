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
        self.people.subscribe_profiles(bae_views::sort_language_on)
    }

    /// This person's restore code, for the QR code and to type, for their own
    /// new device.
    pub async fn generate_restore_code(
        &self,
    ) -> Result<crate::sync::restore_code::RestoreCodeShare, LibraryError> {
        let code = self.store.generate_restore_code().await?;
        Ok(crate::sync::restore_code::RestoreCodeShare::new(code.clone(), code))
    }

    pub fn subscribe_members(
        &self,
    ) -> coven::LiveQuery<bae_people::membership::Members, bae_store::DbError> {
        self.membership.subscribe_members()
    }

    /// Start a pairing for Add Device's sheet: its id and the codes to show,
    /// the code to type grouped in fours.
    pub async fn add_device(&self) -> Result<crate::sync::membership::AddingDevice, LibraryError> {
        let added = self.store.add_device().await?;
        Ok(crate::sync::membership::AddingDevice {
            id: added.id,
            code: crate::sync::restore_code::grouped(&added.shared_code),
            qr_text: added.qr_code,
        })
    }

    /// Where the pairing `pairing` is, until Add Device's sheet closes. While
    /// the status is followed, this device syncs every 5 seconds instead of
    /// every 30, so the sheet hears of the new device's request soon after it
    /// is sent.
    pub fn follow_add_device(
        &self,
        pairing: coven::AddDeviceId,
    ) -> coven::LiveQuery<Option<coven::AddDeviceStatus>, coven::CovenError> {
        let sync_often = self.store.sync_often(crate::bootstrap::WAITING_SCREEN_CHECK_INTERVAL);
        self.store.follow_add_device(pairing).process(move |status| {
            // The subscription owns the request; dropping it ends the request.
            let _sync_often = &sync_often;
            Ok(status)
        })
    }

    pub async fn approve_add_device(
        &self,
        request: &coven::AddDeviceRequest,
    ) -> Result<(), LibraryError> {
        Ok(self.store.approve_add_device(request).await?)
    }

    pub async fn decline_add_device(
        &self,
        request: &coven::AddDeviceRequest,
    ) -> Result<(), LibraryError> {
        Ok(self.store.decline_add_device(request).await?)
    }

    pub async fn end_add_device(&self, pairing: coven::AddDeviceId) -> Result<(), LibraryError> {
        Ok(self.store.end_add_device(pairing).await?)
    }

    /// A request for this device to sync every 5 seconds instead of every 30,
    /// held for as long as an invite's screen shows, so the screen hears of
    /// the invite's person soon after their request is sent. Dropping the
    /// guard ends the request.
    pub fn hold_invite_screen_sync(&self) -> coven::SyncOftenGuard {
        self.store.sync_often(crate::bootstrap::WAITING_SCREEN_CHECK_INTERVAL)
    }

    /// The admin changes this device asked for, each until the person
    /// dismisses its outcome, live (§5.6).
    pub fn subscribe_admin_changes(
        &self,
    ) -> coven::LiveQuery<Vec<bae_people::admin_change::AdminChange>, bae_store::DbError> {
        self.membership.subscribe_admin_changes()
    }

    /// Remove a device. When it is one of this person's own and their account
    /// holds the library's storage, the answer offers the cut-off (coven
    /// §20.9), decided here before anything is removed: a failed read fails the
    /// removal, changing nothing, and the person retries it.
    pub async fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<DeviceRemoval, LibraryError> {
        let own = self.membership.members().await?.members.iter().any(|member| {
            member.is_self && member.active_devices.iter().any(|active| active.id == device)
        });
        let holds_store = own && self.store.holds_store().await?;
        let instruction = self.membership.remove_device(device).await?;
        Ok(DeviceRemoval { offers_cut_off: holds_store && instruction.is_some(), instruction })
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

/// What removing a device left to do.
#[derive(Debug)]
pub struct DeviceRemoval {
    /// The step at the provider, when there is one.
    pub instruction: Option<coven::ProviderSignOut>,
    /// Whether the person may confirm they cut the device off.
    pub offers_cut_off: bool,
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
