//! Who is in the library and on which devices, read from coven's views
//! (`coven_members`, `coven_devices`): each member with their devices that
//! can sync and those that lost access, each list in device-id order.

use super::*;
use crate::membership::{ActiveDevice, FormerDevice, Members, MembershipMember};
use bae_store::read::column_conversion_error;
use bae_store::FormerDeviceState;

impl Membership {
    pub async fn members(&self) -> Result<Members, DbError> {
        self.store.read(|sql| Ok(membership_on(&sql)?)).await
    }

    pub fn subscribe_members(&self) -> coven::LiveQuery<Members> {
        self.store.subscribe(|sql| Ok(membership_on(&sql)?))
    }
}

/// Whether the member using this device is an admin.
pub(crate) const SELF_IS_ADMIN: &str = "SELECT EXISTS(SELECT 1 FROM coven_members \
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
pub(crate) use member_columns;

/// The current members in member-id order.
const MEMBERS: &str = concat!(
    "SELECT ",
    member_columns!(),
    " FROM coven_members AS m WHERE NOT m.removed ORDER BY m.member"
);

fn membership_on(sql: &SqlReadContext<'_>) -> rusqlite::Result<Members> {
    let self_is_admin: bool = sql.query_row(SELF_IS_ADMIN, [], |row| row.get(0))?;
    let members = sql.query(MEMBERS, [], |row| member_row(row, 0, self_is_admin))?;
    Ok(Members { members, self_is_admin })
}

/// The member whose [`member_columns`] start at column `first`.
pub(crate) fn member_row(
    row: &Row<'_>,
    first: usize,
    self_is_admin: bool,
) -> rusqlite::Result<MembershipMember> {
    let is_self: bool = row.get(first + 2)?;
    let role = match row.get::<_, String>(first + 1)?.as_str() {
        "admin" => coven::MemberRole::Admin,
        "member" => coven::MemberRole::Member,
        other => {
            return Err(column_conversion_error(
                row,
                first + 1,
                format!("{other} is not a member role"),
            ))
        }
    };
    let keys = coven::MemberPublicKeys {
        signing: coven::SigningKey::from_bytes(row.get(first + 3)?)
            .map_err(|error| column_conversion_error(row, first + 3, error))?,
        sealing: coven::SealingKey::from_bytes(row.get(first + 4)?),
    };
    let access = match (
        row.get::<_, Option<String>>(first + 5)?,
        row.get::<_, Option<String>>(first + 6)?,
    ) {
        (Some(account), None) => coven::MemberAccess::ProviderAccount(account),
        (None, Some(access_key_id)) => coven::MemberAccess::S3AccessKey { access_key_id },
        _ => {
            return Err(column_conversion_error(
                row,
                first + 5,
                "a member's access holds both or neither of an account and an S3 key id",
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
                    id: device_id(row, first + 7, &id)?,
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
                    id: device_id(row, first + 8, &id)?,
                    name,
                    state: match state.as_str() {
                        "removed" => FormerDeviceState::Removed,
                        "replaced" => FormerDeviceState::Replaced,
                        other => {
                            return Err(column_conversion_error(
                                row,
                                first + 8,
                                format!("{other} is not a device state"),
                            ))
                        }
                    },
                })
            })
            .collect::<rusqlite::Result<_>>()?,
    })
}

/// A device id from the hex of its eight bytes, listed in `row`'s column
/// `index`.
fn device_id(row: &Row<'_>, index: usize, hex: &str) -> rusqlite::Result<coven::DeviceId> {
    u64::from_str_radix(hex, 16)
        .map(coven::DeviceId)
        .map_err(|error| column_conversion_error(row, index, error))
}
