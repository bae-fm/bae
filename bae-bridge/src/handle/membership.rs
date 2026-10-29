use super::*;
use crate::types::*;
use std::sync::Arc;

/// Retains the authenticated request; callers cannot manufacture its authorization.
#[derive(uniffi::Object)]
pub struct BridgeJoinRequest {
    request: coven::JoinRequest,
}
impl BridgeJoinRequest {
    pub(crate) fn new(request: coven::JoinRequest) -> Self {
        Self { request }
    }
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeJoinRequestInfo {
    pub invite: String,
    pub member: String,
    pub keys: BridgeMemberPublicKeys,
    pub device_name: String,
    pub provider_account_email: Option<String>,
    /// The six digits the requester's device shows, as coven writes them
    /// (`041 778`).
    pub confirmation: String,
}
#[uniffi::export]
impl BridgeJoinRequest {
    pub fn info(&self) -> BridgeJoinRequestInfo {
        BridgeJoinRequestInfo {
            invite: self.request.invite.0.to_string(),
            member: self.request.member.0.to_string(),
            keys: BridgeMemberPublicKeys::from_core(self.request.keys),
            device_name: self.request.device_name.clone(),
            provider_account_email: self.request.provider_account_email.clone(),
            confirmation: self.request.confirmation.to_string(),
        }
    }
}
#[uniffi::export(callback_interface)]
pub trait MembershipCallback: Send + Sync {
    fn on_value(&self, value: BridgeMembership);
    fn on_error(&self, error: BridgeError);
}
/// The person's device that joined after the subscription's first value, or
/// none yet.
#[uniffi::export(callback_interface)]
pub trait AddedDeviceCallback: Send + Sync {
    fn on_value(&self, device: Option<BridgeActiveDevice>);
    fn on_error(&self, error: BridgeError);
}
#[uniffi::export(callback_interface)]
pub trait InvitesCallback: Send + Sync {
    fn on_value(&self, value: Vec<BridgeInvite>);
}
#[uniffi::export]
impl AppHandle {
    pub fn subscribe_members(
        &self,
        callback: Box<dyn MembershipCallback>,
    ) -> Arc<crate::LiveSubscription> {
        self.subscribe_query(
            |services| services.subscribe_members(),
            move |result| match result {
                Ok(value) => callback.on_value(BridgeMembership::from_core(value)),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
    /// This person's device that joins after the first value, which is
    /// none: Add Device's sheet watches it while it shows the code.
    pub fn subscribe_added_device(
        &self,
        callback: Box<dyn AddedDeviceCallback>,
    ) -> Arc<crate::LiveSubscription> {
        self.subscribe_query(
            |services| services.subscribe_added_device(),
            move |result| match result {
                Ok(device) => callback.on_value(device.map(BridgeActiveDevice::from_core)),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
    /// This device's open invites, each with its request once one has
    /// arrived; other devices' invites are never listed.
    pub fn subscribe_invites(
        &self,
        callback: Box<dyn InvitesCallback>,
    ) -> Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_invites(),
            move |invites| {
                callback.on_value(
                    invites
                        .iter()
                        .cloned()
                        .map(BridgeInvite::from_core)
                        .collect(),
                )
            },
        )
    }
}
forward! {async this => {
    fn get_members()->BridgeMembership {Ok(BridgeMembership::from_core(this.services.get_members().await?))}
    fn people()->Vec<BridgePerson> {Ok(this.services.people().await?.into_iter().map(BridgePerson::from_core).collect())}
    fn own_person()->Option<BridgePerson> {Ok(this.services.own_person().await?.map(BridgePerson::from_core))}
    fn remove_member(member:String)->() {Ok(this.services.remove_member(&coven::MemberId(member.parse().map_err(BridgeError::config)?)).await?)}
    fn remove_device(device:u64)->Option<BridgeProviderSignOut> {Ok(this.services.remove_device(coven::DeviceId(device)).await?.map(BridgeProviderSignOut::from_core))}
    fn set_member_role(member:String,role:BridgeMemberRole)->() {Ok(this.services.set_member_role(&coven::MemberId(member.parse().map_err(BridgeError::config)?),role.into_core()).await?)}
    fn invite_person(role:BridgeMemberRole,name:String,access:BridgeInviteAccess,delivery:BridgeInviteDelivery)->BridgeInvite {Ok(BridgeInvite::from_core(this.services.invite_person(role.into_core(),name,access.into_core(),delivery.into_core()).await?))}
    fn approve_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.approve_join_request(&request.request).await?)}
    fn decline_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.decline_join_request(&request.request).await?)}
    fn cancel_invite(invite:String)->() {Ok(this.services.cancel_invite(&coven::InviteId(invite.parse().map_err(BridgeError::config)?)).await?)}
} }
