//! The admin changes this device asked for (§5.6): what each changes, in
//! `admin_operations`, written in the write that journals the change, and
//! how coven's `coven_operations` lists its operation. The write that
//! acknowledges a change's outcome deletes its row.

use super::*;
use crate::admin_change::{
    AdminChange, AdminChangeDismissError, AdminChangeKind, AdminChangeState,
};

/// What an admin change changes, as its `admin_operations` row holds it.
pub(super) enum AdminTarget {
    LibraryName(String),
    Role(coven::MemberId, coven::MemberRole),
    Person(coven::MemberId),
    Device(coven::DeviceId),
    /// The member a request joins as, and the name its invite gave them.
    Approval(coven::MemberId, Option<String>),
}

/// Each change with its operation as coven lists it, the person's profile
/// name and the device's name, in the order they were asked for.
const ADMIN_CHANGES_SQL: &str = "SELECT a.operation, o.operation IS NOT NULL, o.kind, o.outcome, \
         a.member, a.device, a.role, a.name, p.name, d.name \
     FROM admin_operations AS a \
     LEFT JOIN coven_operations AS o ON o.operation = a.operation \
     LEFT JOIN people AS p ON p.id = a.member \
     LEFT JOIN coven_devices AS d ON d.device = a.device \
     ORDER BY a.operation";

fn admin_changes_on(sql: &impl coven::SqlRead) -> rusqlite::Result<Vec<AdminChange>> {
    sql.query(ADMIN_CHANGES_SQL, [], |row| {
        let operation = coven::OperationId(row.get(0)?);
        // Only the write that deletes the row acknowledges its operation.
        if !row.get::<_, bool>(1)? {
            return Err(bae_store::read::column_conversion_error(
                row,
                0,
                format!("the admin change's operation {operation:?} isn't listed"),
            ));
        }
        let member = || {
            row.get::<_, Option<coven::MemberId>>(4)?.ok_or_else(|| {
                bae_store::read::column_conversion_error(row, 4, "a member change names no member")
            })
        };
        let change = match row.get::<_, coven::OperationKind>(2)? {
            coven::OperationKind::RenameStore => AdminChangeKind::RenameLibrary {
                name: row.get::<_, Option<String>>(7)?.ok_or_else(|| {
                    bae_store::read::column_conversion_error(row, 7, "a rename names no name")
                })?,
            },
            coven::OperationKind::SetRole => AdminChangeKind::SetRole {
                member: member()?,
                person: row.get(8)?,
                role: match row.get::<_, Option<String>>(6)?.as_deref() {
                    Some("admin") => coven::MemberRole::Admin,
                    Some("member") => coven::MemberRole::Member,
                    other => {
                        return Err(bae_store::read::column_conversion_error(
                            row,
                            6,
                            format!("a role {other:?}"),
                        ))
                    }
                },
            },
            coven::OperationKind::RemoveMember => {
                AdminChangeKind::RemovePerson { member: member()?, person: row.get(8)? }
            }
            coven::OperationKind::Invite | coven::OperationKind::ReplaceMemberKeys => {
                AdminChangeKind::Approve {
                    member: member()?,
                    person: match row.get::<_, Option<String>>(7)? {
                        Some(invitee) => Some(invitee),
                        None => row.get(8)?,
                    },
                }
            }
            coven::OperationKind::RemoveDevice => AdminChangeKind::RemoveDevice {
                device: row.get::<_, Option<coven::DeviceId>>(5)?.ok_or_else(|| {
                    bae_store::read::column_conversion_error(row, 5, "a removal names no device")
                })?,
                // coven journals the removal of a device this device
                // hasn't read, which it lists only once it has.
                name: row.get(9)?,
            },
            other => {
                return Err(bae_store::read::column_conversion_error(
                    row,
                    2,
                    format!("an admin change of kind {other:?}"),
                ))
            }
        };
        let state = match row.get::<_, Option<coven::OperationOutcome>>(3)? {
            None => AdminChangeState::Waiting,
            Some(coven::OperationOutcome::Landed(_) | coven::OperationOutcome::Satisfied) => {
                AdminChangeState::Done
            }
            Some(coven::OperationOutcome::Rejected(reason)) => AdminChangeState::Refused(reason),
            Some(coven::OperationOutcome::Stopped(stop)) => AdminChangeState::Stopped(stop),
            // Only storage setup is cancelled or undone, never an admin change.
            Some(
                outcome @ (coven::OperationOutcome::Cancelled | coven::OperationOutcome::Undone(_)),
            ) => {
                return Err(bae_store::read::column_conversion_error(
                    row,
                    3,
                    format!("an admin change that ended {outcome:?}"),
                ))
            }
        };
        Ok(AdminChange { operation, change, state })
    })
}

/// Record what the change journaled as `operation` changes, in its write.
pub(super) fn insert_admin_operation(
    sql: &CovenWrite<'_>,
    operation: coven::OperationId,
    target: &AdminTarget,
) -> rusqlite::Result<()> {
    let role = |role: &coven::MemberRole| match role {
        coven::MemberRole::Admin => "admin",
        coven::MemberRole::Member => "member",
    };
    let (member, device, role, name) = match target {
        AdminTarget::LibraryName(name) => (None, None, None, Some(name.as_str())),
        AdminTarget::Role(member, chosen) => (Some(*member), None, Some(role(chosen)), None),
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

    /// The admin changes this device asked for, live: each until its outcome
    /// is acknowledged.
    pub fn subscribe_admin_changes(
        &self,
    ) -> coven::LiveQuery<Vec<AdminChange>, bae_store::DbError> {
        self.store.subscribe(|sql| Ok(admin_changes_on(&sql)?))
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

impl Membership {
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
    /// which a pass binds, and records it to show by the name the invite
    /// gave its person (§5.6).
    pub async fn approve_join_request(
        &self,
        request: &coven::JoinRequest,
    ) -> Result<(), coven::SyncError> {
        let invite = request.invite;
        let invitee = self
            .store
            .read_coven(coven::Invite::list)
            .await?
            .into_iter()
            .find(|open| open.id == invite)
            .and_then(|open| open.invitee);
        let request = request.clone();
        self.admin_change(AdminTarget::Approval(request.member, invitee), move |sql| {
            Ok((Some(sql.approve_join_request(&request)?), ()))
        })
        .await
    }
}
