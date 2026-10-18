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
            let person =
                people
                    .entry(membership.member.id)
                    .or_insert_with(|| crate::people::Person {
                        id: membership.member.id,
                        profile: None,
                        membership: None,
                    });
            person.membership = Some(membership);
        }
        Ok(people.into_values().collect())
    }

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
