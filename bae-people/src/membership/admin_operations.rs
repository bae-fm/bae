//! The admin changes this device asked for (§5.6): what each changes, in
//! `admin_operations`, written in the write that journals the change, and
//! how coven's `coven_operations` lists its operation. The write that
//! acknowledges a change's outcome deletes its row.

use super::*;
use crate::admin_change::{
    AdminChange, AdminChangeDismissError, AdminChangeKind, AdminChangePlace, AdminChangeState,
    AdminChanges,
};

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

/// The operation kinds some place shows. A change whose operation coven lists
/// with another kind is no admin change bae makes.
const PLACED_KINDS: &str =
    "'RenameStore', 'SetRole', 'RemoveMember', 'RemoveDevice', 'ApproveJoin', 'ReplaceMemberKeys'";

/// Each admin change this device asked for, with its operation as coven
/// lists it. `coven_operations` lists a change's operation until the write
/// that deletes the change's row acknowledges it.
pub(super) const ADMIN_CHANGES_FROM: &str = "FROM admin_operations AS a \
     JOIN coven_operations AS o ON o.operation = a.operation";

/// The changes that show in `place`, each with its operation as coven lists
/// it, the person's profile name and the device's name, in the order this
/// device asked for them.
fn admin_changes_sql(place: &AdminChangePlace) -> String {
    format!(
        "SELECT a.operation, o.kind, o.outcome, \
                a.member, a.device, a.role, a.name, p.name, d.name \
         {ADMIN_CHANGES_FROM} \
         LEFT JOIN people AS p ON p.id = a.member \
         LEFT JOIN coven_devices AS d ON d.device = a.device \
         WHERE {shown} \
         ORDER BY a.operation",
        shown = shown_in(place),
    )
}

/// Whether any change shows in `place`, as [`admin_changes_sql`] returns
/// them.
pub(super) fn any_admin_change_sql(place: &AdminChangePlace) -> String {
    format!("SELECT EXISTS (SELECT 1 {ADMIN_CHANGES_FROM} WHERE {shown})", shown = shown_in(place))
}

/// The condition, over [`ADMIN_CHANGES_FROM`], that keeps the changes that
/// show in `place`: those of the kinds the place shows, and every change
/// whose operation coven lists with a kind no place shows, so that the row
/// reader fails the read in every place.
pub(super) fn shown_in(place: &AdminChangePlace) -> String {
    let kinds = match place {
        AdminChangePlace::Library => "o.kind = 'RenameStore'",
        AdminChangePlace::Person(_) => "o.kind = 'SetRole' AND a.member = ?1",
        AdminChangePlace::People => {
            "o.kind IN ('RemoveMember', 'RemoveDevice', 'ApproveJoin', 'ReplaceMemberKeys')"
        }
    };
    format!("(o.kind NOT IN ({PLACED_KINDS}) OR ({kinds}))")
}

fn admin_changes_on(
    sql: &impl coven::SqlRead,
    place: &AdminChangePlace,
) -> rusqlite::Result<AdminChanges> {
    let member = match place {
        AdminChangePlace::Person(member) => Some(*member),
        AdminChangePlace::Library | AdminChangePlace::People => None,
    };
    let changes = sql.query(
        &admin_changes_sql(place),
        rusqlite::params_from_iter(member.iter()),
        admin_change_row,
    )?;
    let any = sql.query_row(
        &any_admin_change_sql(place),
        rusqlite::params_from_iter(member.iter()),
        |row| row.get(0),
    )?;
    Ok(AdminChanges { changes, any })
}

/// One change as [`admin_changes_sql`] reads it.
fn admin_change_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AdminChange> {
    let operation = coven::OperationId(row.get(0)?);
    let member = || {
        row.get::<_, Option<coven::MemberId>>(3)?.ok_or_else(|| {
            bae_store::read::column_conversion_error(row, 3, "a member change names no member")
        })
    };
    let kind = row.get::<_, coven::OperationKind>(1)?;
    let change = match kind {
        coven::OperationKind::RenameStore => AdminChangeKind::RenameLibrary {
            name: row.get::<_, Option<String>>(6)?.ok_or_else(|| {
                bae_store::read::column_conversion_error(row, 6, "a rename names no name")
            })?,
        },
        coven::OperationKind::SetRole => AdminChangeKind::SetRole {
            member: member()?,
            person: row.get(7)?,
            role: row.get::<_, Option<coven::MemberRole>>(5)?.ok_or_else(|| {
                bae_store::read::column_conversion_error(row, 5, "a role change names no role")
            })?,
        },
        coven::OperationKind::RemoveMember => {
            AdminChangeKind::RemovePerson { member: member()?, person: row.get(7)? }
        }
        coven::OperationKind::ApproveJoin | coven::OperationKind::ReplaceMemberKeys => {
            AdminChangeKind::Approve {
                member: member()?,
                person: match row.get::<_, Option<String>>(6)? {
                    Some(invitee) => Some(invitee),
                    None => row.get(7)?,
                },
            }
        }
        coven::OperationKind::RemoveDevice => AdminChangeKind::RemoveDevice {
            device: row.get::<_, Option<coven::DeviceId>>(4)?.ok_or_else(|| {
                bae_store::read::column_conversion_error(row, 4, "a removal names no device")
            })?,
            // coven journals the removal of a device this device
            // hasn't read, which it lists only once it has.
            name: row.get(8)?,
        },
        other => {
            return Err(bae_store::read::column_conversion_error(
                row,
                1,
                format!("an admin change of kind {other:?}"),
            ))
        }
    };
    let outcome = match kind {
        coven::OperationKind::ReplaceMemberKeys => {
            row.get::<_, Option<coven::FreshChangeOutcome>>(2)?.map(fresh_change_outcome)
        }
        // RenameStore, SetRole, RemoveMember, RemoveDevice and ApproveJoin,
        // the other kinds the match above reads.
        _ => row.get::<_, Option<coven::ChangeOutcome>>(2)?,
    };
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
        self.store.subscribe(move |sql| Ok(admin_changes_on(&sql, &place)?))
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
