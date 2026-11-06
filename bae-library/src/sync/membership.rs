//! Add Device's watch for the person's device that joins from its code.

use bae_people::membership::{ActiveDevice, Members};

/// Watches the membership for a device of this device's person that wasn't
/// among their active devices when Add Device made its code. Add Device's
/// sheet shows that code until one joins.
///
/// The devices the person had are read before the code is made: a
/// subscription's first result can come after a device restored from the code
/// and this device applied its registration.
#[derive(Clone, Debug)]
pub struct AddedDeviceWatch {
    /// The person's active devices before the code was made.
    known: std::collections::BTreeSet<coven::DeviceId>,
}

impl AddedDeviceWatch {
    /// Watch for a device of the person's that isn't among their active
    /// devices in `membership`.
    pub fn new(membership: &Members) -> Self {
        Self {
            known: membership
                .members
                .iter()
                .filter(|member| member.is_self)
                .flat_map(|member| &member.active_devices)
                .map(|device| device.id)
                .collect(),
        }
    }

    /// The first of the person's other active devices, in device-id order,
    /// that wasn't known; none until one joins.
    pub fn added(&self, membership: &Members) -> Option<ActiveDevice> {
        membership
            .members
            .iter()
            .find(|member| member.is_self)?
            .active_devices
            .iter()
            .find(|device| !device.is_self && !self.known.contains(&device.id))
            .cloned()
    }
}

/// What Add Device's sheet shows and watches: the person's restore code, and
/// the watch for their device that joins from it.
#[derive(Debug)]
pub struct AddingDevice {
    pub share: super::restore_code::RestoreCodeShare,
    pub watch: AddedDeviceWatch,
}

#[cfg(test)]
#[path = "membership_tests.rs"]
mod tests;
