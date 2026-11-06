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
/// The person's devices before Add Device made its code; a subscription
/// reports the first of theirs that joins after.
#[derive(uniffi::Object)]
pub struct BridgeAddedDeviceWatch {
    watch: bae_library::sync::membership::AddedDeviceWatch,
}
impl BridgeAddedDeviceWatch {
    pub(crate) fn new(watch: bae_library::sync::membership::AddedDeviceWatch) -> Self {
        Self { watch }
    }
}
/// What Add Device's sheet shows, and the watch it runs while it does.
#[derive(Clone, uniffi::Record)]
pub struct BridgeAddDevice {
    pub share: BridgeRestoreCodeShare,
    pub watch: Arc<BridgeAddedDeviceWatch>,
}
impl BridgeAddDevice {
    fn from_core(value: bae_library::sync::membership::AddingDevice) -> Self {
        Self {
            share: BridgeRestoreCodeShare::from_core(value.share),
            watch: Arc::new(BridgeAddedDeviceWatch::new(value.watch)),
        }
    }
}
#[uniffi::export]
impl AppHandle {
    /// The store's members and devices, now and after each change.
    pub fn subscribe_members(&self) -> Arc<crate::MembersSubscription> {
        crate::screens::members(self.services.subscribe_members(), self.runtime.clone())
    }
    /// This person's device that `watch` finds: none until one joins after
    /// Add Device made its code. Add Device's sheet pulls it while it shows
    /// the code.
    pub fn subscribe_added_device(
        &self,
        watch: Arc<BridgeAddedDeviceWatch>,
    ) -> Arc<crate::AddedDeviceSubscription> {
        crate::screens::added_device(
            self.services.subscribe_added_device(watch.watch.clone()),
            self.runtime.clone(),
        )
    }
    /// The admin changes this device asked for, each until the person
    /// dismisses its outcome.
    pub fn subscribe_admin_changes(&self) -> Arc<crate::AdminChangesSubscription> {
        crate::screens::admin_changes(self.services.subscribe_admin_changes(), self.runtime.clone())
    }
    /// This device's open invites, each with its stage and what coven holds
    /// its work for; other devices' invites are never listed.
    pub fn subscribe_invites(&self) -> Arc<crate::InvitesSubscription> {
        crate::screens::invites(self.services.subscribe_invites(), self.runtime.clone())
    }
}
forward! {async this => {
    fn add_device()->BridgeAddDevice {Ok(BridgeAddDevice::from_core(this.services.add_device().await?))}
    fn get_members()->BridgeMembership {Ok(BridgeMembership::from_core(this.services.get_members().await?))}
    fn people()->Vec<BridgePerson> {Ok(this.services.people().await?.into_iter().map(BridgePerson::from_core).collect())}
    fn own_person()->Option<BridgePerson> {Ok(this.services.own_person().await?.map(BridgePerson::from_core))}
    fn remove_member(member:String)->() {Ok(this.services.remove_member(&crate::from_app::member(&member)?).await?)}
    fn remove_device(device:u64)->Option<BridgeProviderSignOut> {Ok(this.services.remove_device(coven::DeviceId(device)).await?.map(BridgeProviderSignOut::from_core))}
    fn acknowledge_admin_change(operation:i64)->() {Ok(this.services.acknowledge_admin_change(coven::OperationId(operation)).await?)}
    fn set_member_role(member:String,role:BridgeMemberRole)->() {Ok(this.services.set_member_role(&crate::from_app::member(&member)?,role.into_core()).await?)}
    fn invite_person(role:BridgeMemberRole,name:String,access:BridgeInviteAccess,lifetime:BridgeInviteLifetime)->String {Ok(this.services.invite_person(role.into_core(),name,access.into_core(),lifetime.into_core()).await?.0.to_string())}
    fn approve_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.approve_join_request(&request.request).await?)}
    fn decline_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.decline_join_request(&request.request).await?)}
    fn cancel_invite(invite:String)->() {Ok(this.services.cancel_invite(&coven::InviteId(invite.parse().map_err(BridgeError::config)?)).await?)}
} }
