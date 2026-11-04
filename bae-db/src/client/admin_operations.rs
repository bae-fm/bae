//! The admin changes this device asked for (§5.6): what each changes, in
//! `admin_operations`, written in the write that journals the change, and
//! how coven's `coven_operations` lists its operation. The write that
//! acknowledges a change's outcome deletes its row.

use super::*;
use crate::admin_change::{AdminChange, AdminChangeKind, AdminChangeState};

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
            return Err(super::read::column_conversion_error(
                row,
                0,
                format!("the admin change's operation {operation:?} isn't listed"),
            ));
        }
        let member = || {
            row.get::<_, Option<coven::MemberId>>(4)?.ok_or_else(|| {
                super::read::column_conversion_error(row, 4, "a member change names no member")
            })
        };
        let change = match row.get::<_, coven::OperationKind>(2)? {
            coven::OperationKind::RenameStore => AdminChangeKind::RenameLibrary {
                name: row.get::<_, Option<String>>(7)?.ok_or_else(|| {
                    super::read::column_conversion_error(row, 7, "a rename names no name")
                })?,
            },
            coven::OperationKind::SetRole => AdminChangeKind::SetRole {
                member: member()?,
                person: row.get(8)?,
                role: match row.get::<_, Option<String>>(6)?.as_deref() {
                    Some("admin") => coven::MemberRole::Admin,
                    Some("member") => coven::MemberRole::Member,
                    other => {
                        return Err(super::read::column_conversion_error(
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
                    super::read::column_conversion_error(row, 5, "a removal names no device")
                })?,
                // coven journals the removal of a device this device
                // hasn't read, which it lists only once it has.
                name: row.get(9)?,
            },
            other => {
                return Err(super::read::column_conversion_error(
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

impl Database {
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
        self.inner
            .handle
            .write(move |sql| {
                let (operation, value) = change(&sql)?;
                if let Some(operation) = operation {
                    insert_admin_operation(&sql, operation, &target)?;
                }
                Ok(value)
            })
            .await
            .map_err(coven::SyncError::from)
    }

    /// The admin changes this device asked for, live: each until its outcome
    /// is acknowledged.
    pub fn subscribe_admin_changes(&self) -> coven::LiveQuery<Vec<AdminChange>> {
        self.inner.handle.subscribe(|sql| Ok(admin_changes_on(&sql)?))
    }

    /// Acknowledge a finished admin change's outcome, so it leaves the list:
    /// coven's operation and its row go in one write. One still running is
    /// refused with coven's `OperationRunning`.
    pub async fn acknowledge_admin_change(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), DbError> {
        self.call(move |sql| {
            sql.acknowledge_operation(operation)?;
            let deleted =
                sql.execute("DELETE FROM admin_operations WHERE operation = ?1", [operation.0])?;
            if deleted != 1 {
                return Err(DbError::Message(format!(
                    "operation {operation:?} is no admin change of this device's"
                )));
            }
            Ok(())
        })
        .await
    }
}
