//! Who is in the library and on which devices, read from coven's views
//! (`coven_members`, `coven_devices`): each member with their devices that
//! can sync and those that lost access, each list in device-id order.

use super::fragments::{CurrentMembersParams, Member};
use super::*;
use crate::membership::{ActiveDevice, FormerDevice, Members, MembershipMember};
use bae_store::{statement_rows, FormerDeviceState};

impl Membership {
    pub async fn members(&self) -> Result<Members, DbError> {
        self.store.read(|sql| membership_on(&sql)).await
    }

    pub fn subscribe_members(&self) -> coven::LiveQuery<Members, bae_store::DbError> {
        self.store.subscribe(|sql| membership_on(&sql))
    }

    /// This device's name, as Your devices lists it.
    pub async fn this_device_name(&self) -> Result<String, DbError> {
        self.store.read(|sql| Ok(sql.query_row(THIS_DEVICE_NAME, [], |row| row.get(0))?)).await
    }
}

/// The name of this device's own registration.
const THIS_DEVICE_NAME: &str = "SELECT name FROM coven_devices WHERE is_self AND state = 'active'";

fn membership_on(sql: &SqlReadContext<'_>) -> Result<Members, DbError> {
    let rows = statement_rows(sql, &CurrentMembersParams {})?;
    // Each row says whether this device's member is an admin. An admin is a
    // current member, so a read that returns no row has none.
    let self_is_admin = rows.first().is_some_and(|row| row.member.viewer.self_is_admin);
    let members =
        rows.into_iter().map(|row| row.member.into_membership()).collect::<Result<_, _>>()?;
    Ok(Members { members, self_is_admin })
}

impl Member {
    /// The member as the people screens show them, with the removals this
    /// device's person may make.
    pub(crate) fn into_membership(self) -> Result<MembershipMember, DbError> {
        let id = self.member_id;
        let self_is_admin = self.viewer.self_is_admin;
        let is_self = self.is_self;
        let access = coven::MemberAccess::from_columns(self.account, self.access_key_id).map_err(
            |error| DbError::unreadable(format!("the access of member {id}"), error.to_string()),
        )?;
        Ok(MembershipMember {
            id,
            role: self.role,
            access,
            is_self,
            can_remove: MembershipMember::removable(self_is_admin, is_self),
            active_device_count: self.active_device_count,
            former_device_count: self.former_device_count,
            active_devices: self
                .active_devices
                .into_iter()
                .map(|device| {
                    Ok(ActiveDevice {
                        id: device_id(&device.id)?,
                        name: device.name,
                        is_self: device.is_self,
                        can_remove: ActiveDevice::removable(self_is_admin, is_self, device.is_self),
                    })
                })
                .collect::<Result<_, DbError>>()?,
            // The fragment reads no `is_self` of a former device: this
            // installation's own lost registration reads as removed or
            // replaced like any other.
            former_devices: self
                .former_devices
                .into_iter()
                .map(|device| {
                    Ok(FormerDevice {
                        id: device_id(&device.id)?,
                        name: device.name,
                        state: former_state(&device.state)?,
                    })
                })
                .collect::<Result<_, DbError>>()?,
        })
    }
}

/// How a device in the list of former devices lost access, from coven's name
/// of its `state`. The fragment's SQL keeps active devices out of that list.
fn former_state(state: &str) -> Result<FormerDeviceState, DbError> {
    let unreadable =
        |cause: String| DbError::unreadable(format!("a former device's state {state:?}"), cause);
    let parsed =
        state.parse::<coven::DeviceState>().map_err(|error| unreadable(error.to_string()))?;
    FormerDeviceState::of(parsed)
        .ok_or_else(|| unreadable("the list of former devices holds an active one".to_owned()))
}

/// A device id from its eight bytes.
pub(super) fn device_id(bytes: &[u8]) -> Result<coven::DeviceId, DbError> {
    <[u8; 8]>::try_from(bytes).map(|bytes| coven::DeviceId(u64::from_be_bytes(bytes))).map_err(
        |error| {
            DbError::unreadable(format!("a device id of {} bytes", bytes.len()), error.to_string())
        },
    )
}
