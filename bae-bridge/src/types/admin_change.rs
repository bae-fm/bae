/// An admin change this device asked for, until the person dismisses its
/// outcome (§5.6).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeAdminChange {
    /// The change's operation, which dismissing it names.
    pub operation: i64,
    pub change: BridgeAdminChangeKind,
    pub state: BridgeAdminChangeState,
}
/// What an admin change changes. A person without a profile has no name, nor
/// does a device this device hasn't read yet.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAdminChangeKind {
    RenameLibrary { name: String },
    SetRole { member: String, person: Option<String>, role: super::BridgeMemberRole },
    RemovePerson { member: String, person: Option<String> },
    RemoveDevice { device: u64, name: Option<String> },
    Approve { member: String, person: Option<String> },
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAdminChangeState {
    /// Recorded on this device; sync hasn't published it yet.
    Waiting,
    /// The library has it.
    Done,
    /// The library refused it; nothing changed.
    Refused { reason: super::BridgeOperationRejection },
    /// This device stopped for good before the change was made.
    Stopped { stop: super::BridgeDeviceStop },
}
impl BridgeAdminChange {
    pub(crate) fn from_core(value: bae_db::admin_change::AdminChange) -> Self {
        use bae_db::admin_change::{AdminChangeKind, AdminChangeState};
        let bae_db::admin_change::AdminChange { operation, change, state } = value;
        Self {
            operation: operation.0,
            change: match change {
                AdminChangeKind::RenameLibrary { name } => {
                    BridgeAdminChangeKind::RenameLibrary { name }
                }
                AdminChangeKind::SetRole { member, person, role } => {
                    BridgeAdminChangeKind::SetRole {
                        member: member.0.to_string(),
                        person,
                        role: super::BridgeMemberRole::from_core(role),
                    }
                }
                AdminChangeKind::RemovePerson { member, person } => {
                    BridgeAdminChangeKind::RemovePerson { member: member.0.to_string(), person }
                }
                AdminChangeKind::Approve { member, person } => {
                    BridgeAdminChangeKind::Approve { member: member.0.to_string(), person }
                }
                AdminChangeKind::RemoveDevice { device, name } => {
                    BridgeAdminChangeKind::RemoveDevice { device: device.0, name }
                }
            },
            state: match state {
                AdminChangeState::Waiting => BridgeAdminChangeState::Waiting,
                AdminChangeState::Done => BridgeAdminChangeState::Done,
                AdminChangeState::Refused(reason) => BridgeAdminChangeState::Refused {
                    reason: super::BridgeOperationRejection::from_core(reason),
                },
                AdminChangeState::Stopped(stop) => BridgeAdminChangeState::Stopped {
                    stop: super::BridgeDeviceStop::from_core(stop),
                },
            },
        }
    }
}
