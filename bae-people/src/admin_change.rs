//! The admin changes this device asked for, as the library shows them until
//! the person dismisses each one's outcome (§5.6).

/// An admin change this device asked for: what it changes, and how it
/// stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminChange {
    /// Its operation in coven's list, which dismissing it acknowledges.
    pub operation: coven::OperationId,
    pub change: AdminChangeKind,
    pub state: AdminChangeState,
}

/// The admin changes that show in one place, and whether the place has any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminChanges {
    pub changes: Vec<AdminChange>,
    /// Whether the place has a change, which the read's own statement
    /// answers, so a screen draws the place's section from it.
    pub any: bool,
}

/// What an admin change changes. A person is named by their profile, which
/// they may not have made; a device keeps its name after its removal, and
/// one this device hasn't read yet has no name here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminChangeKind {
    RenameLibrary {
        name: String,
    },
    SetRole {
        member: coven::MemberId,
        person: Option<String>,
        role: coven::MemberRole,
    },
    RemovePerson {
        member: coven::MemberId,
        person: Option<String>,
    },
    RemoveDevice {
        device: coven::DeviceId,
        name: Option<String>,
    },
    /// Letting in the person an invite's request names: by the name
    /// the invite gave them, or for a replacement invite, which names
    /// nobody, their profile's.
    Approve {
        member: coven::MemberId,
        person: Option<String>,
    },
}

/// Where an admin change shows: next to what it changes. A rename shows with
/// the library's name, a role change on that person's page, and removing a
/// person or a device and letting a person in on People and devices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminChangePlace {
    Library,
    Person(coven::MemberId),
    People,
}

/// How an admin change stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminChangeState {
    /// Recorded on this device; sync hasn't published it yet.
    Waiting,
    /// The library has it: sync published it, or found it already made.
    Done,
    /// The library refused it once sync had caught up; nothing changed.
    Refused(coven::OperationRejection),
    /// This device stopped for good before the change was made, so it never
    /// will be from here. Nothing refused it: another device may still make
    /// it.
    Stopped(coven::DeviceStop),
}

/// Why the person's dismissal of an admin change was refused.
#[derive(Debug, thiserror::Error)]
pub enum AdminChangeDismissError {
    /// The operation is no admin change this device asked for, or one
    /// dismissed already. Nothing was acknowledged.
    #[error("operation {0:?} is no admin change of this device's")]
    NotAdminChange(coven::OperationId),
    #[error(transparent)]
    Database(#[from] bae_store::DbError),
}

/// A bug the store's write carried back, which the store reported as it
/// returned; a refusal holds none.
impl bae_util::task_failure::ReportedBug for AdminChangeDismissError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::NotAdminChange(_) => None,
        }
    }
}
