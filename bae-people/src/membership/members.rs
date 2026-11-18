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

    pub fn subscribe_members(&self) -> coven::LiveQuery<Members, bae_store::DbError> {
        self.store.subscribe(|sql| Ok(membership_on(&sql)?))
    }

    /// This device's name, as Your devices lists it.
    pub async fn this_device_name(&self) -> Result<String, DbError> {
        self.store.read(|sql| Ok(sql.query_row(THIS_DEVICE_NAME, [], |row| row.get(0))?)).await
    }
}

/// The name of this device's own registration.
const THIS_DEVICE_NAME: &str = "SELECT name FROM coven_devices WHERE is_self AND state = 'active'";

/// Whether the member using this device is an admin, as SQL.
macro_rules! self_is_admin {
    () => {
        "EXISTS(SELECT 1 FROM coven_members AS s \
         WHERE s.is_self AND NOT s.removed AND s.role = 'admin')"
    };
}

/// The columns [`member_row`] reads of the `coven_members` row `m`: the
/// member, with their active devices and their removed or replaced ones as
/// JSON arrays in device-id order, how many of each they have, and whether
/// the member using this device is an admin. A device id is its eight bytes
/// as hex.
macro_rules! member_columns {
    () => {
        concat!(
            "m.member, m.role, m.is_self, m.account, m.access_key_id, \
         (SELECT json_group_array(json_array(hex(d.device), d.name, \
                     json(CASE WHEN d.is_self THEN 'true' ELSE 'false' END)) \
                     ORDER BY d.device) \
          FROM coven_devices AS d WHERE d.member = m.member AND d.state = 'active'), \
         (SELECT json_group_array(json_array(hex(d.device), d.name, d.state) \
                     ORDER BY d.device) \
          FROM coven_devices AS d WHERE d.member = m.member AND d.state <> 'active'), \
         (SELECT COUNT(*) FROM coven_devices AS d \
          WHERE d.member = m.member AND d.state = 'active'), \
         (SELECT COUNT(*) FROM coven_devices AS d \
          WHERE d.member = m.member AND d.state <> 'active'), \
         ",
            $crate::membership::self_is_admin!()
        )
    };
}
pub(crate) use member_columns;
pub(crate) use self_is_admin;

/// The current members in member-id order.
const MEMBERS: &str = concat!(
    "SELECT ",
    member_columns!(),
    " FROM coven_members AS m WHERE NOT m.removed ORDER BY m.member"
);

fn membership_on(sql: &SqlReadContext<'_>) -> rusqlite::Result<Members> {
    let self_is_admin: bool = sql.query_row(SELF_IS_ADMIN, [], |row| row.get(0))?;
    let members = sql.query(MEMBERS, [], |row| member_row(row, 0))?;
    Ok(Members { members, self_is_admin })
}

/// The list's own read of whether the member using this device is an admin.
const SELF_IS_ADMIN: &str = concat!("SELECT ", self_is_admin!());

/// Whether the member using this device is an admin, as the row whose
/// [`member_columns`] start at column `first` holds it.
fn self_is_admin_column(row: &Row<'_>, first: usize) -> rusqlite::Result<bool> {
    row.get(first + 9)
}

/// The member whose [`member_columns`] start at column `first`.
pub(crate) fn member_row(row: &Row<'_>, first: usize) -> rusqlite::Result<MembershipMember> {
    let self_is_admin = self_is_admin_column(row, first)?;
    let is_self: bool = row.get(first + 2)?;
    let access = coven::MemberAccess::from_columns(row.get(first + 3)?, row.get(first + 4)?)
        .map_err(|error| column_conversion_error(row, first + 3, error))?;
    let active: Vec<(String, String, bool)> = json_column(row, first + 5)?;
    let former: Vec<(String, String, String)> = json_column(row, first + 6)?;
    Ok(MembershipMember {
        id: row.get(first)?,
        role: row.get(first + 1)?,
        access,
        is_self,
        can_remove: MembershipMember::removable(self_is_admin, is_self),
        active_device_count: row.get(first + 7)?,
        former_device_count: row.get(first + 8)?,
        active_devices: active
            .into_iter()
            .map(|(id, name, device_is_self)| {
                Ok(ActiveDevice {
                    id: device_id(row, first + 5, &id)?,
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
                    id: device_id(row, first + 6, &id)?,
                    name,
                    state: former_state(row, first + 6, &state)?,
                })
            })
            .collect::<rusqlite::Result<_>>()?,
    })
}

/// How a device in the list of former devices in `row`'s column `index`
/// lost access, from coven's name of its `state`. The list's query keeps
/// active devices out of it.
fn former_state(row: &Row<'_>, index: usize, state: &str) -> rusqlite::Result<FormerDeviceState> {
    let state = state.parse().map_err(|error| column_conversion_error(row, index, error))?;
    FormerDeviceState::of(state).ok_or_else(|| {
        column_conversion_error(row, index, "the list of former devices holds an active one")
    })
}

/// A device id from the hex of its eight bytes, listed in `row`'s column
/// `index`.
fn device_id(row: &Row<'_>, index: usize, hex: &str) -> rusqlite::Result<coven::DeviceId> {
    u64::from_str_radix(hex, 16)
        .map(coven::DeviceId)
        .map_err(|error| column_conversion_error(row, index, error))
}
