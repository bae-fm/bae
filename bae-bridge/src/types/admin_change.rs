/// An admin change this device asked for, until the person dismisses its
/// outcome (§5.6).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeAdminChange {
    /// The change's operation, which dismissing it names.
    pub operation: i64,
    pub change: BridgeAdminChangeKind,
    pub state: BridgeAdminChangeState,
}
/// The admin changes that show in one place, and whether the place has any,
/// which the read's statement answers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeAdminChanges {
    pub changes: Vec<BridgeAdminChange>,
    pub any: bool,
}
impl BridgeAdminChanges {
    pub(crate) fn from_core(value: bae_people::admin_change::AdminChanges) -> Self {
        Self {
            changes: value.changes.into_iter().map(BridgeAdminChange::from_core).collect(),
            any: value.any,
        }
    }
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
/// Where an admin change shows: next to what it changes. A rename shows with
/// the library's name, a role change on that person's page by member id, and
/// removing a person or a device and letting a person in on People and
/// devices.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeAdminChangePlace {
    Library,
    Person { member: String },
    People,
}
impl BridgeAdminChangePlace {
    /// The place the app names, with a member id that is no member id
    /// refused as a configuration error.
    pub(crate) fn into_core(
        self,
    ) -> Result<bae_people::admin_change::AdminChangePlace, super::BridgeError> {
        use bae_people::admin_change::AdminChangePlace;
        Ok(match self {
            Self::Library => AdminChangePlace::Library,
            Self::Person { member } => AdminChangePlace::Person(crate::from_app::member(&member)?),
            Self::People => AdminChangePlace::People,
        })
    }
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
    pub(crate) fn from_core(value: bae_people::admin_change::AdminChange) -> Self {
        use bae_people::admin_change::{AdminChangeKind, AdminChangeState};
        let bae_people::admin_change::AdminChange { operation, change, state } = value;
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
