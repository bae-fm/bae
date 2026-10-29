//! Membership belongs to people; each person can register several devices.

use super::*;

impl LibraryManager {
    pub async fn create_profile(
        &self,
        name: &str,
    ) -> Result<crate::people::PersonProfile, crate::people::ProfileError> {
        self.database.create_profile(name).await
    }

    pub async fn rename_profile(&self, name: &str) -> Result<(), crate::people::ProfileError> {
        self.database.rename_profile(name).await
    }

    pub async fn set_history_sharing(
        &self,
        sharing: crate::people::Sharing,
    ) -> Result<(), crate::people::ProfileError> {
        self.database.set_history_sharing(sharing).await
    }

    pub async fn set_avatar(
        &self,
        bytes: Option<Vec<u8>>,
    ) -> Result<(), crate::people::ProfileError> {
        let avatar = bytes.map(|bytes| (self.ids.new_id().to_string(), bytes));
        self.database.set_avatar(avatar).await
    }

    pub async fn avatar(
        &self,
        person: coven::MemberId,
    ) -> Result<Option<Vec<u8>>, crate::people::ProfileError> {
        self.database.avatar(person).await
    }

    pub fn subscribe_profiles(&self) -> coven::LiveQuery<Vec<crate::people::PersonProfile>> {
        self.database.subscribe_profiles()
    }

    pub async fn people(&self) -> Result<Vec<crate::people::Person>, LibraryError> {
        let mut people = std::collections::BTreeMap::new();
        for profile in self.database.profiles().await? {
            people.insert(
                profile.id,
                crate::people::Person {
                    id: profile.id,
                    profile: Some(profile),
                    membership: None,
                },
            );
        }
        for membership in self.get_members().await?.members {
            let person = people
                .entry(membership.id)
                .or_insert_with(|| crate::people::Person {
                    id: membership.id,
                    profile: None,
                    membership: None,
                });
            person.membership = Some(membership);
        }
        Ok(people.into_values().collect())
    }

    /// This person's restore code, and for the QR code the one that holds, on
    /// S3, the key this device holds, for their own new device.
    pub async fn generate_restore_code(
        &self,
    ) -> Result<crate::sync::restore_code::RestoreCodeShare, LibraryError> {
        let code = self.database.generate_restore_code().await?;
        let qr_text = self.database.restore_code_for_transfer().await?;
        crate::sync::restore_code::RestoreCodeShare::new(code, qr_text)
            .map_err(LibraryError::TransferCode)
    }

    pub async fn get_members(&self) -> Result<crate::sync::membership::Membership, LibraryError> {
        Ok(self.database.membership().await?)
    }

    pub fn subscribe_members(&self) -> coven::LiveQuery<crate::sync::membership::Membership> {
        self.database.subscribe_membership()
    }

    /// This person's device that joins after the subscription's first
    /// result: none until one does. Add Device's sheet watches it while it
    /// shows the code.
    pub fn subscribe_added_device(
        &self,
    ) -> coven::LiveQuery<Option<crate::sync::membership::ActiveDevice>> {
        let watch = crate::sync::membership::AddedDeviceWatch::default();
        self.subscribe_members()
            .process(move |membership| Ok(watch.added(&membership)))
    }

    pub async fn remove_member(&self, member: &coven::MemberId) -> Result<(), LibraryError> {
        Ok(self.database.remove_member(member).await?)
    }

    pub async fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<Option<coven::ProviderSignOut>, LibraryError> {
        Ok(self.database.remove_device(device).await?)
    }

    pub async fn set_member_role(
        &self,
        member: &coven::MemberId,
        role: coven::MemberRole,
    ) -> Result<(), LibraryError> {
        Ok(self.database.set_member_role(member, role).await?)
    }

    /// Re-inviting creates a fresh identity; the removed person's rows retain
    /// their owner. `name` is the profile name the person's device writes
    /// when it joins (§5.4).
    pub async fn invite_person(
        &self,
        role: coven::MemberRole,
        name: String,
        access: coven::InviteAccess,
        delivery: coven::InviteDelivery,
    ) -> Result<coven::Invite, LibraryError> {
        Ok(self
            .database
            .invite_person(role, name, access, delivery)
            .await?)
    }

    /// This device's open invites, each with its request once one has arrived.
    pub fn subscribe_invites(&self) -> tokio::sync::watch::Receiver<Vec<coven::Invite>> {
        self.database.subscribe_invites()
    }

    pub async fn approve_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), LibraryError> {
        Ok(self.database.approve_join_request(request).await?)
    }

    pub async fn decline_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), LibraryError> {
        Ok(self.database.decline_join_request(request).await?)
    }

    /// Cancel an invite and take back its access. Once approval of its
    /// person has begun, the invite can't be cancelled (§5.6).
    pub async fn cancel_invite(&self, invite: &coven::InviteId) -> Result<(), LibraryError> {
        match self.database.cancel_invite(invite).await {
            Err(error @ coven::SyncError::InvitationChanged) => {
                Err(LibraryError::InviteJoining(error))
            }
            result => Ok(result?),
        }
    }
}
