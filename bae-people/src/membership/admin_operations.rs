//! The admin changes this device asked for (§5.6): what each changes, in
//! `admin_operations`, written in the write that journals the change, and
//! how coven's `coven_operations` lists its operation. The write that
//! acknowledges a change's outcome deletes its row.

use super::*;
use crate::admin_change::{
    AdminChange, AdminChangeDismissError, AdminChangeKind, AdminChangePlace, AdminChangeState,
    AdminChanges,
};
use bae_store::statement_rows;
use rusqlite::types::{FromSql, ValueRef};

/// What an admin change changes, as its `admin_operations` row holds it.
pub(super) enum AdminTarget {
    LibraryName(String),
    Role(coven::MemberId, coven::MemberRole),
    Person(coven::MemberId),
    Device(coven::DeviceId),
    /// The member a request joins as, and the name their request carried;
    /// none for a recovery, whose profile already has one.
    Approval(coven::MemberId, Option<String>),
}

// Each admin change this device asked for, with its operation as coven lists
// it, the person's profile, the device's name and the place that shows it.
// `coven_operations` lists a change's operation until the write that deletes
// the change's row acknowledges it. A change whose operation coven lists with
// a kind no place shows has no place.
sqlive_compose::fragment!(
    AdminOperation {
        kind: coven::OperationKind,
        member: Option<coven::MemberId>,
        device: Option<coven::DeviceId>,
        role: Option<coven::MemberRole>,
        place: Option<String>
    },
    r#"
    SELECT a.operation, o.kind, o.outcome, a.member, a.device, a.role, a.name,
           person.*, d.name AS device_name,
           CASE WHEN o.kind = 'RenameStore' THEN 'library'
                WHEN o.kind = 'SetRole' THEN 'person'
                WHEN o.kind IN ('RemoveMember', 'RemoveDevice', 'ApproveJoin',
                                'ReplaceMemberKeys') THEN 'people'
           END AS place
    FROM admin_operations AS a
    JOIN coven_operations AS o ON o.operation = a.operation
    LEFT JOIN @crate::people::profiles::Profile AS person ON person.id = a.member
    LEFT JOIN coven_devices AS d ON d.device = a.device
"#
);

// The reads of each place's changes, in the order this device asked for them.
// Each also returns a change with no place, so that the row's decoding fails
// the read wherever the person looks.

sqlive_compose::read!(
    LibraryAdminChanges,
    r#"
    SELECT change.*
    FROM @crate::membership::admin_operations::AdminOperation AS change
    WHERE change.place IS NULL OR change.place = 'library'
    ORDER BY change.operation
"#
);

sqlive_compose::read!(
    PersonAdminChanges(member: coven::MemberId),
    r#"
    SELECT change.*
    FROM @crate::membership::admin_operations::AdminOperation AS change
    WHERE change.place IS NULL OR (change.place = 'person' AND change.member = :member)
    ORDER BY change.operation
"#
);

sqlive_compose::read!(
    PeopleAdminChanges,
    r#"
    SELECT change.*
    FROM @crate::membership::admin_operations::AdminOperation AS change
    WHERE change.place IS NULL OR change.place = 'people'
    ORDER BY change.operation
"#
);

fn admin_changes_on(
    sql: &impl coven::SqlRead,
    place: &AdminChangePlace,
) -> Result<AdminChanges, DbError> {
    let operations: Vec<AdminOperation> = match place {
        AdminChangePlace::Library => statement_rows(sql, &LibraryAdminChangesParams {})?
            .into_iter()
            .map(|row| row.change)
            .collect(),
        AdminChangePlace::Person(member) => {
            statement_rows(sql, &PersonAdminChangesParams { member: *member })?
                .into_iter()
                .map(|row| row.change)
                .collect()
        }
        AdminChangePlace::People => statement_rows(sql, &PeopleAdminChangesParams {})?
            .into_iter()
            .map(|row| row.change)
            .collect(),
    };
    let changes =
        operations.into_iter().map(AdminOperation::into_change).collect::<Result<Vec<_>, _>>()?;
    let any = !changes.is_empty();
    Ok(AdminChanges { changes, any })
}

impl AdminOperation {
    /// The change as its place shows it.
    fn into_change(self) -> Result<AdminChange, DbError> {
        let operation = coven::OperationId(self.operation);
        let unreadable = |cause: String| {
            DbError::unreadable(format!("the admin change of operation {}", operation.0), cause)
        };
        let missing = |what: &str| unreadable(what.to_owned());
        let member = self.member;
        let member = || member.ok_or_else(|| missing("a member change names no member"));
        let profile_name = self.person.map(|profile| profile.name);
        let change = match self.kind {
            coven::OperationKind::RenameStore => AdminChangeKind::RenameLibrary {
                name: self.name.ok_or_else(|| missing("a rename names no name"))?,
            },
            coven::OperationKind::SetRole => AdminChangeKind::SetRole {
                member: member()?,
                person: profile_name,
                role: self.role.ok_or_else(|| missing("a role change names no role"))?,
            },
            coven::OperationKind::RemoveMember => {
                AdminChangeKind::RemovePerson { member: member()?, person: profile_name }
            }
            coven::OperationKind::ApproveJoin | coven::OperationKind::ReplaceMemberKeys => {
                AdminChangeKind::Approve { member: member()?, person: self.name.or(profile_name) }
            }
            coven::OperationKind::RemoveDevice => AdminChangeKind::RemoveDevice {
                device: self.device.ok_or_else(|| missing("a removal names no device"))?,
                // coven journals the removal of a device this device
                // hasn't read, which it lists only once it has.
                name: self.device_name,
            },
            other => return Err(unreadable(format!("an admin change of kind {other:?}"))),
        };
        let outcome = self
            .outcome
            .map(|stored| {
                let stored = ValueRef::Blob(&stored);
                match self.kind {
                    coven::OperationKind::ReplaceMemberKeys => {
                        coven::FreshChangeOutcome::column_result(stored).map(fresh_change_outcome)
                    }
                    // RenameStore, SetRole, RemoveMember, RemoveDevice and
                    // ApproveJoin, the other kinds the match above reads.
                    _ => coven::ChangeOutcome::column_result(stored),
                }
            })
            .transpose()
            .map_err(|error| unreadable(error.to_string()))?;
        let state = match outcome {
            None => AdminChangeState::Waiting,
            Some(coven::ChangeOutcome::Landed(_) | coven::ChangeOutcome::Satisfied) => {
                AdminChangeState::Done
            }
            Some(coven::ChangeOutcome::Rejected(reason)) => AdminChangeState::Refused(reason),
            Some(coven::ChangeOutcome::Stopped(stop)) => AdminChangeState::Stopped(stop),
        };
        Ok(AdminChange { operation, change, state })
    }
}

/// How a member's key replacement ended, as an admin change ends: it is
/// never satisfied, which `ChangeOutcome` also holds.
fn fresh_change_outcome(outcome: coven::FreshChangeOutcome) -> coven::ChangeOutcome {
    match outcome {
        coven::FreshChangeOutcome::Landed(slot) => coven::ChangeOutcome::Landed(slot),
        coven::FreshChangeOutcome::Rejected(reason) => coven::ChangeOutcome::Rejected(reason),
        coven::FreshChangeOutcome::Stopped(stop) => coven::ChangeOutcome::Stopped(stop),
    }
}

/// Record what the change journaled as `operation` changes, in its write.
pub(super) fn insert_admin_operation(
    sql: &CovenWrite<'_>,
    operation: coven::OperationId,
    target: &AdminTarget,
) -> rusqlite::Result<()> {
    let (member, device, role, name) = match target {
        AdminTarget::LibraryName(name) => (None, None, None, Some(name.as_str())),
        AdminTarget::Role(member, role) => (Some(*member), None, Some(*role), None),
        AdminTarget::Person(member) => (Some(*member), None, None, None),
        AdminTarget::Device(device) => (None, Some(*device), None, None),
        AdminTarget::Approval(member, invitee) => (Some(*member), None, None, invitee.as_deref()),
    };
    sql.execute(
        "INSERT INTO admin_operations (operation, member, device, role, name) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![operation.0, member, device, role, name],
    )?;
    Ok(())
}

impl Membership {
    /// Make an admin change in a write of its own: `change` runs coven's step
    /// and the write records what it changes, when it journals an operation
    /// (none when the library already has it), so the change and its row
    /// commit together.
    pub(super) async fn admin_change<R: Send + 'static>(
        &self,
        target: AdminTarget,
        change: impl FnOnce(&CovenWrite<'_>) -> Result<(Option<coven::OperationId>, R), coven::SyncError>
            + Send
            + 'static,
    ) -> Result<R, coven::SyncError> {
        self.store
            .write(move |sql| {
                let (operation, value) = change(&sql)?;
                if let Some(operation) = operation {
                    insert_admin_operation(&sql, operation, &target)?;
                }
                Ok(value)
            })
            .await
    }

    /// The admin changes this device asked for that show in `place`, live:
    /// each until the person acknowledges its outcome.
    pub fn subscribe_admin_changes(
        &self,
        place: AdminChangePlace,
    ) -> coven::LiveQuery<AdminChanges, bae_store::DbError> {
        self.store.subscribe(move |sql| admin_changes_on(&sql, &place))
    }

    /// Acknowledge a finished admin change's outcome, so it leaves the list:
    /// coven's operation and its row go in one write. One still running is
    /// refused with coven's `OperationRunning`, and an operation that is no
    /// admin change of this device's as `NotAdminChange`.
    pub async fn acknowledge_admin_change(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), AdminChangeDismissError> {
        let acknowledged = self
            .store
            .call(move |sql| {
                let deleted = sql
                    .execute("DELETE FROM admin_operations WHERE operation = ?1", [operation.0])?;
                if deleted == 0 {
                    return Ok(false);
                }
                sql.acknowledge_operation(operation)?;
                Ok(true)
            })
            .await?;
        match acknowledged {
            true => Ok(()),
            false => Err(AdminChangeDismissError::NotAdminChange(operation)),
        }
    }
}

/// Why the person's dismissal of an invite a sharing refusal ended was
/// refused.
#[derive(Debug, thiserror::Error)]
pub enum RefusedInviteDismissError {
    /// The operation is no invite's that a refused sharing ended, or one
    /// dismissed already. Nothing was acknowledged.
    #[error("operation {0:?} is no invite a sharing refusal ended")]
    NotRefusedInvite(coven::OperationId),
    #[error(transparent)]
    Database(#[from] bae_store::DbError),
}

/// A bug the store's write carried back, which the store reported as it
/// returned; a refusal holds none.
impl bae_util::task_failure::ReportedBug for RefusedInviteDismissError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::NotRefusedInvite(_) => None,
        }
    }
}

/// Why an invite couldn't be made.
#[derive(Debug, thiserror::Error)]
pub enum InviteError {
    /// This device's person has no name: no profile, no request for one and
    /// no name kept from creating the library. No path makes such a person,
    /// so this is a bug of bae's.
    #[error("the inviting person has no name")]
    InviterUnnamed,
    #[error(transparent)]
    Database(#[from] bae_store::DbError),
    #[error(transparent)]
    Sync(#[from] coven::SyncError),
}

impl Membership {
    /// Invite a new person, in the role and for the lifetime chosen, with the
    /// access chosen.
    /// The invite names its inviter by the name this person goes by, which
    /// the joining person's screens show; it carries no name for the invited
    /// person, who types theirs (§5.4, §5.6).
    pub async fn invite_person(
        &self,
        role: coven::MemberRole,
        access: coven::InviteAccess,
        lifetime: coven::InviteLifetime,
    ) -> Result<coven::InviteId, InviteError> {
        let inviter = self
            .store
            .read(|sql| Ok(crate::people::name_this_person_goes_by(&sql)?))
            .await?
            .ok_or(InviteError::InviterUnnamed)?;
        Ok(self.store.invite_person(role, access, lifetime, inviter).await?)
    }
}

impl Membership {
    /// Dismiss an invite a sharing refusal ended: coven's operation outcome
    /// is acknowledged, so the notice leaves the list in that write.
    /// `coven_operations` lists an invite only once a sharing refusal ends
    /// it, so any other operation is refused with `NotRefusedInvite`, as is
    /// one dismissed already.
    pub async fn dismiss_refused_invite(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), RefusedInviteDismissError> {
        let acknowledged = self
            .store
            .call(move |sql| {
                let refused = sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM coven_operations \
                     WHERE operation = ?1 AND kind = 'Invite')",
                    [operation.0],
                    |row| row.get::<_, bool>(0),
                )?;
                if refused {
                    sql.acknowledge_operation(operation)?;
                }
                Ok(refused)
            })
            .await?;
        match acknowledged {
            true => Ok(()),
            false => Err(RefusedInviteDismissError::NotRefusedInvite(operation)),
        }
    }

    /// Rename the store as an admin: the write journals the rename, which
    /// a pass claims, and records it to show (§5.6).
    pub async fn rename_store(&self, name: &str) -> Result<(), coven::SyncError> {
        let name = name.to_owned();
        let renamed = name.clone();
        self.admin_change(AdminTarget::LibraryName(renamed), move |sql| {
            Ok((sql.rename_store(&name)?, ()))
        })
        .await
    }

    /// Remove a member: the write journals the removal, which a pass
    /// claims, and records it to show (§5.6).
    pub async fn remove_member(&self, member: &coven::MemberId) -> Result<(), coven::SyncError> {
        let member = *member;
        self.admin_change(AdminTarget::Person(member), move |sql| {
            Ok((sql.remove_member(&member)?, ()))
        })
        .await
    }

    /// Remove a device, journaled and recorded as a removal is, and say
    /// what is left to do at the provider.
    pub async fn remove_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<Option<coven::ProviderSignOut>, coven::SyncError> {
        self.admin_change(AdminTarget::Device(device), move |sql| sql.remove_device(device)).await
    }

    /// Change a member's role, journaled and recorded as a removal is.
    pub async fn set_member_role(
        &self,
        member: &coven::MemberId,
        role: coven::MemberRole,
    ) -> Result<(), coven::SyncError> {
        let member = *member;
        self.admin_change(AdminTarget::Role(member, role), move |sql| {
            Ok((sql.set_member_role(&member, role)?, ()))
        })
        .await
    }

    /// Approve an invite's request: the write journals the approval,
    /// which a pass binds, and records it to show by the name the person
    /// typed, which their request carries; a member's new-keys request carries
    /// none, and the row reads their profile (§5.6).
    pub async fn approve_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), coven::SyncError> {
        let name = match &request.joiner {
            coven::Joiner::NewPerson { name } => Some(name.clone()),
            coven::Joiner::MemberKeys => None,
        };
        let request = request.clone();
        self.admin_change(AdminTarget::Approval(request.member, name), move |sql| {
            Ok((Some(sql.approve_join_request(&request)?), ()))
        })
        .await
    }
}
