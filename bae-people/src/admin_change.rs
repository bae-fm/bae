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
    /// Letting in the person a sent invite's request names: by the name
    /// the invite gave them, or for a replacement invite, which names
    /// nobody, their profile's.
    Approve {
        member: coven::MemberId,
        person: Option<String>,
    },
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
