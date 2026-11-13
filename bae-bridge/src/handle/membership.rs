use super::*;
use crate::types::*;
use std::sync::Arc;

/// Retains the authenticated request; callers cannot manufacture its authorization.
#[derive(uniffi::Object)]
pub struct BridgeJoinRequest {
    request: coven::JoinRequest,
    /// The name the approval shows: the one the person typed when they
    /// joined, or, for a member getting access again, their profile's.
    name: String,
}
impl BridgeJoinRequest {
    pub(crate) fn new(request: coven::JoinRequest, name: String) -> Self {
        Self { request, name }
    }
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeJoinRequestInfo {
    pub invite: String,
    pub member: String,
    pub keys: BridgeMemberPublicKeys,
    pub device_name: String,
    /// The name the person typed when they joined, which the request
    /// carries; the approval's title.
    pub name: String,
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
            name: self.name.clone(),
            provider_account_email: self.request.provider_account_email.clone(),
            confirmation: self.request.confirmation.to_string(),
        }
    }
}
/// This device's request to sync every 5 seconds, held while an invite's
/// screen shows. The screen releases it as it goes.
#[derive(uniffi::Object)]
pub struct BridgeInviteScreenHold {
    guard: std::sync::Mutex<Option<coven::SyncOftenGuard>>,
}
#[uniffi::export]
impl BridgeInviteScreenHold {
    /// End the request. Releasing again does nothing.
    pub fn release(&self) {
        self.guard.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    }
}
/// What Add Device's sheet shows of the pairing it started: its id, which
/// names it in every later call, the code to type or copy, and the code for
/// the QR, which on S3 holds this device's key.
#[derive(Clone, uniffi::Record)]
pub struct BridgeAddDevice {
    pub id: String,
    pub code: String,
    pub qr_text: String,
}
impl BridgeAddDevice {
    fn from_core(value: bae_library::sync::membership::AddingDevice) -> Self {
        Self { id: value.id.0.to_string(), code: value.code, qr_text: value.qr_text }
    }
}
/// Retains the checked request of a new device to be added; callers cannot
/// manufacture its authorization.
#[derive(uniffi::Object)]
pub struct BridgeAddDeviceRequest {
    request: coven::AddDeviceRequest,
}
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAddDeviceRequestInfo {
    /// The name the new device gave itself.
    pub device_name: String,
    /// The six digits the new device shows, as coven writes them (`041 778`).
    pub confirmation: String,
}
#[uniffi::export]
impl BridgeAddDeviceRequest {
    pub fn info(&self) -> BridgeAddDeviceRequestInfo {
        BridgeAddDeviceRequestInfo {
            device_name: self.request.device_name.clone(),
            confirmation: self.request.confirmation.to_string(),
        }
    }
}
/// Where a pairing is. Add Device's sheet offers approving and declining
/// only for a waiting request.
#[derive(Clone, uniffi::Enum)]
pub enum BridgeAddDeviceStatus {
    /// Waiting for a request, then for the person's decision on it.
    Waiting {
        request: Option<Arc<BridgeAddDeviceRequest>>,
    },
    /// The person approved the named device's request; it is on its way in.
    Approved {
        device_name: String,
    },
    Ended {
        end: BridgeAddDeviceEnd,
    },
}
/// How a pairing ended.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeAddDeviceEnd {
    /// The new device is in, under `name`.
    Added {
        name: String,
    },
    Declined,
    /// The request failed one of coven's checks.
    Refused,
    /// The code's 30 minutes passed.
    Expired,
}
impl BridgeAddDeviceStatus {
    pub(crate) fn from_core(value: coven::AddDeviceStatus) -> Self {
        match value {
            coven::AddDeviceStatus::Waiting { request } => Self::Waiting {
                request: request.map(|request| Arc::new(BridgeAddDeviceRequest { request })),
            },
            coven::AddDeviceStatus::Approved { request } => {
                Self::Approved { device_name: request.device_name }
            }
            coven::AddDeviceStatus::Ended(end) => Self::Ended {
                end: match end {
                    coven::AddDeviceEnd::Added { name, .. } => BridgeAddDeviceEnd::Added { name },
                    coven::AddDeviceEnd::Declined => BridgeAddDeviceEnd::Declined,
                    coven::AddDeviceEnd::Refused => BridgeAddDeviceEnd::Refused,
                    coven::AddDeviceEnd::Expired => BridgeAddDeviceEnd::Expired,
                },
            },
        }
    }
}
#[uniffi::export]
impl AppHandle {
    /// The store's members and devices, now and after each change.
    pub fn subscribe_members(&self) -> Arc<crate::MembersSubscription> {
        crate::screens::members(self.services.subscribe_members(), self.runtime.clone())
    }
    /// The pairing `pairing` started, now and after each change, until its
    /// subscription is cancelled. While it lives this device syncs every 5
    /// seconds, so the sheet hears of a new device's request soon after it
    /// is sent. The sheet that showed the code ends the pairing when it
    /// closes (`end_add_device`).
    pub fn follow_add_device(
        &self,
        pairing: String,
    ) -> Result<Arc<crate::AddDeviceStatusSubscription>, BridgeError> {
        let pairing = coven::AddDeviceId(pairing.parse().map_err(BridgeError::config)?);
        Ok(crate::screens::add_device_status(
            self.services.follow_add_device(pairing),
            self.runtime.clone(),
        ))
    }
    /// Asks this device to sync every 5 seconds until the returned hold is
    /// released: an invite's screen holds it while it shows.
    pub fn hold_invite_screen_sync(&self) -> Arc<BridgeInviteScreenHold> {
        Arc::new(BridgeInviteScreenHold {
            guard: std::sync::Mutex::new(Some(self.services.hold_invite_screen_sync())),
        })
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
    fn remove_device(device:u64)->BridgeDeviceRemoval {Ok(BridgeDeviceRemoval::from_core(this.services.remove_device(coven::DeviceId(device)).await?))}
    fn confirm_device_cut_off(device:u64)->BridgeDeviceCutOff {Ok(BridgeDeviceCutOff::from_core(this.services.confirm_device_cut_off(coven::DeviceId(device)).await?))}
    fn acknowledge_admin_change(operation:i64)->() {Ok(this.services.acknowledge_admin_change(coven::OperationId(operation)).await?)}
    fn set_member_role(member:String,role:BridgeMemberRole)->() {Ok(this.services.set_member_role(&crate::from_app::member(&member)?,role.into_core()).await?)}
    fn invite_person(role:BridgeMemberRole,access:BridgeInviteAccess,lifetime:BridgeInviteLifetime)->String {Ok(this.services.invite_person(role.into_core(),access.into_core(),lifetime.into_core()).await?.0.to_string())}
    fn dismiss_refused_invite(operation:i64)->() {Ok(this.services.dismiss_refused_invite(coven::OperationId(operation)).await?)}
    fn approve_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.approve_join_request(&request.request).await?)}
    fn decline_join_request(request:Arc<BridgeJoinRequest>)->() {Ok(this.services.decline_join_request(&request.request).await?)}
    fn approve_add_device(request:Arc<BridgeAddDeviceRequest>)->() {Ok(this.services.approve_add_device(&request.request).await?)}
    fn decline_add_device(request:Arc<BridgeAddDeviceRequest>)->() {Ok(this.services.decline_add_device(&request.request).await?)}
    fn end_add_device(pairing:String)->() {Ok(this.services.end_add_device(coven::AddDeviceId(pairing.parse().map_err(BridgeError::config)?)).await?)}
    fn cancel_invite(invite:String)->() {Ok(this.services.cancel_invite(&coven::InviteId(invite.parse().map_err(BridgeError::config)?)).await?)}
} }
