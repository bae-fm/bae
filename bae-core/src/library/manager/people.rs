//! Membership belongs to people; each person can register several devices.

use super::*;

impl LibraryManager {
    pub async fn generate_restore_code(&self) -> Result<String, LibraryError> {
        Ok(self.database.generate_restore_code().await?)
    }

    pub async fn get_members(&self) -> Result<crate::sync::membership::Membership, LibraryError> {
        Ok(crate::sync::membership::Membership::from_members(
            self.database.members().await?,
        ))
    }

    pub fn subscribe_members(&self) -> coven::LiveQuery<crate::sync::membership::Membership> {
        self.database
            .subscribe_members()
            .process(|members| Ok(crate::sync::membership::Membership::from_members(members)))
    }

    pub async fn remove_member(&self, member: &coven::MemberId) -> Result<(), LibraryError> {
        Ok(self.database.remove_member(member).await?)
    }

    pub async fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<coven::ProviderSignOut, LibraryError> {
        Ok(self.database.remove_device(device).await?)
    }

    pub async fn set_member_role(
        &self,
        member: &coven::MemberId,
        role: coven::MemberRole,
    ) -> Result<(), LibraryError> {
        Ok(self.database.set_member_role(member, role).await?)
    }

    /// Re-inviting creates a fresh identity; the removed person's rows retain their owner.
    pub async fn invite_person(
        &self,
        role: coven::MemberRole,
        access: coven::InviteAccess,
    ) -> Result<coven::Invite, LibraryError> {
        Ok(self.database.invite_person(role, access).await?)
    }

    pub fn subscribe_join_requests(&self) -> tokio::sync::watch::Receiver<Vec<coven::JoinRequest>> {
        self.database.subscribe_join_requests()
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

    pub async fn cancel_invite(&self, invite: &coven::InviteId) -> Result<(), LibraryError> {
        Ok(self.database.cancel_invite(invite).await?)
    }
}
