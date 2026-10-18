use super::*;
use crate::types::*;
use std::sync::Arc;

/// Retains the authenticated request; callers cannot manufacture its authorization.
#[derive(uniffi::Object)]
pub struct BridgeJoinRequest {
    request: coven::JoinRequest,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeJoinRequestInfo {
    pub invite: String,
    pub member: String,
    pub keys: BridgeMemberPublicKeys,
    pub device_name: String,
    pub provider_account_email: Option<String>,
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
        }
    }
}
#[uniffi::export(callback_interface)]
pub trait MembershipCallback: Send + Sync {
    fn on_value(&self, value: BridgeMembership);
    fn on_error(&self, error: BridgeError);
}
#[uniffi::export(callback_interface)]
pub trait JoinRequestsCallback: Send + Sync {
    fn on_value(&self, value: Vec<Arc<BridgeJoinRequest>>);
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
    pub fn subscribe_join_requests(
        &self,
        callback: Box<dyn JoinRequestsCallback>,
    ) -> Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_join_requests(),
            move |requests| {
                callback.on_value(
                    requests
                        .iter()
                        .cloned()
                        .map(|request| Arc::new(BridgeJoinRequest { request }))
                        .collect(),
                )
            },
        )
    }
}
forward! {async this => {
    fn get_members()->BridgeMembership {Ok(BridgeMembership::from_core(this.services.get_members().await?))}
    fn people()->Vec<BridgePerson> {Ok(this.services.people().await?.into_iter().map(BridgePerson::from_core).collect())}
    fn remove_member(member:String)->() {Ok(this.services.remove_member(&coven::MemberId(member.parse().map_err(BridgeError::config)?)).await?)}
    fn remove_device(device:u64)->BridgeProviderSignOut {Ok(BridgeProviderSignOut::from_core(this.services.remove_device(coven::DeviceId(device)).await?))}
    fn set_member_role(member:String,role:BridgeMemberRole)->() {Ok(this.services.set_member_role(&coven::MemberId(member.parse().map_err(BridgeError::config)?),role.into_core()).await?)}
    fn invite_person(role:BridgeMemberRole,access:BridgeInviteAccess)->BridgeInvite {Ok(BridgeInvite::from_core(this.services.invite_person(role.into_core(),access.into_core()).await?))}
    fn approve_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.approve_join_request(&request.request).await?)}
    fn decline_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.decline_join_request(&request.request).await?)}
    fn cancel_invite(invite:String)->() {Ok(this.services.cancel_invite(&coven::InviteId(invite.parse().map_err(BridgeError::config)?)).await?)}
} }
