//! Who is in the library and on which devices, read from coven's views
//! (`coven_members`, `coven_devices`): each member with their devices that
//! can sync and those that lost access, each list in device-id order.

use super::*;
use crate::sync::membership::{
    ActiveDevice, FormerDevice, FormerDeviceState, Membership, MembershipMember,
};
use rusqlite::types::Type;

impl Database {
    pub async fn membership(&self) -> Result<Membership, DbError> {
        self.read(|sql| Ok(membership_on(&sql)?)).await
    }

    pub(crate) fn subscribe_membership(&self) -> coven::LiveQuery<Membership> {
        self.inner.handle.subscribe(|sql| Ok(membership_on(&sql)?))
    }
}

/// Whether the member using this device is an admin.
pub(super) const SELF_IS_ADMIN: &str = "SELECT EXISTS(SELECT 1 FROM coven_members \
     WHERE is_self AND NOT removed AND role = 'admin')";

/// The columns [`member_row`] reads of the `coven_members` row `m`: the
/// member, with their active devices and their removed or replaced ones as
/// JSON arrays in device-id order. A device id is its eight bytes as hex.
macro_rules! member_columns {
    () => {
        "m.member, m.role, m.is_self, m.signing, m.sealing, m.account, m.access_key_id, \
         (SELECT json_group_array(json_array(hex(d.device), d.name, \
                     json(CASE WHEN d.is_self THEN 'true' ELSE 'false' END)) \
                     ORDER BY d.device) \
          FROM coven_devices AS d WHERE d.member = m.member AND d.state = 'active'), \
         (SELECT json_group_array(json_array(hex(d.device), d.name, d.state) \
                     ORDER BY d.device) \
          FROM coven_devices AS d WHERE d.member = m.member AND d.state <> 'active')"
    };
}
pub(super) use member_columns;

/// The current members in member-id order.
const MEMBERS: &str = concat!(
    "SELECT ",
    member_columns!(),
    " FROM coven_members AS m WHERE NOT m.removed ORDER BY m.member"
);

fn membership_on(sql: &SqlReadContext<'_>) -> rusqlite::Result<Membership> {
    let self_is_admin: bool = sql.query_row(SELF_IS_ADMIN, [], |row| row.get(0))?;
    let members = sql.query(MEMBERS, [], |row| member_row(row, 0, self_is_admin))?;
    Ok(Membership {
        members,
        self_is_admin,
    })
}

/// The member whose [`member_columns`] start at column `first`.
pub(super) fn member_row(
    row: &Row<'_>,
    first: usize,
    self_is_admin: bool,
) -> rusqlite::Result<MembershipMember> {
    let is_self: bool = row.get(first + 2)?;
    let role = match row.get::<_, String>(first + 1)?.as_str() {
        "admin" => coven::MemberRole::Admin,
        "member" => coven::MemberRole::Member,
        other => {
            return Err(unreadable_column(
                first + 1,
                Type::Text,
                "a member role",
                other,
            ))
        }
    };
    let keys = coven::MemberPublicKeys {
        signing: coven::SigningKey::from_bytes(row.get(first + 3)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(first + 3, Type::Blob, Box::new(error))
        })?,
        sealing: coven::SealingKey::from_bytes(row.get(first + 4)?),
    };
    let access = match (
        row.get::<_, Option<String>>(first + 5)?,
        row.get::<_, Option<String>>(first + 6)?,
    ) {
        (Some(account), None) => coven::MemberAccess::ProviderAccount(account),
        (None, Some(access_key_id)) => coven::MemberAccess::S3AccessKey { access_key_id },
        _ => {
            return Err(unreadable_column(
                first + 5,
                Type::Text,
                "a member's access",
                "both or neither of an account and an S3 key id",
            ))
        }
    };
    let active: Vec<(String, String, bool)> = json_column(row, first + 7)?;
    let former: Vec<(String, String, String)> = json_column(row, first + 8)?;
    Ok(MembershipMember {
        id: row.get(first)?,
        keys,
        role,
        access,
        is_self,
        can_remove: MembershipMember::removable(self_is_admin, is_self),
        active_devices: active
            .into_iter()
            .map(|(id, name, device_is_self)| {
                Ok(ActiveDevice {
                    id: device_id(&id, first + 7)?,
                    name,
                    is_self: device_is_self,
                    can_remove: ActiveDevice::removable(self_is_admin, is_self, device_is_self),
                })
            })
            .collect::<rusqlite::Result<_>>()?,
        // A former device's `is_self` is dropped: this installation's own
        // lost registration reads as removed or replaced like any other.
        former_devices: former
            .into_iter()
            .map(|(id, name, state)| {
                Ok(FormerDevice {
                    id: device_id(&id, first + 8)?,
                    name,
                    state: match state.as_str() {
                        "removed" => FormerDeviceState::Removed,
                        "replaced" => FormerDeviceState::Replaced,
                        other => {
                            return Err(unreadable_column(
                                first + 8,
                                Type::Text,
                                "a device state",
                                other,
                            ))
                        }
                    },
                })
            })
            .collect::<rusqlite::Result<_>>()?,
    })
}

fn json_column<T: serde::de::DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    serde_json::from_str(&row.get::<_, String>(index)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

/// A device id from the hex of its eight bytes.
fn device_id(hex: &str, index: usize) -> rusqlite::Result<coven::DeviceId> {
    u64::from_str_radix(hex, 16)
        .map(coven::DeviceId)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
}

fn unreadable_column(index: usize, kind: Type, what: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, kind, format!("{value} is not {what}").into())
}
