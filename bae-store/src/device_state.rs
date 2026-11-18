//! How a device lost access to the library.

/// How a device lost access to the library: coven's states of a device other
/// than active, for the lists and lines that name a device that can no
/// longer sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormerDeviceState {
    Removed,
    Replaced,
}

impl FormerDeviceState {
    /// How a device in coven's `state` lost access, or `None` for an active
    /// device.
    pub fn of(state: coven::DeviceState) -> Option<Self> {
        match state {
            coven::DeviceState::Active => None,
            coven::DeviceState::Removed => Some(Self::Removed),
            coven::DeviceState::Replaced => Some(Self::Replaced),
        }
    }

    /// coven's state of a device that lost access this way.
    pub fn device_state(self) -> coven::DeviceState {
        match self {
            Self::Removed => coven::DeviceState::Removed,
            Self::Replaced => coven::DeviceState::Replaced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FormerDeviceState;

    /// Every former state reads back from coven's state of it, and an active
    /// device has no former state.
    #[test]
    fn a_former_state_reads_back_from_coven_s_state() {
        for state in [FormerDeviceState::Removed, FormerDeviceState::Replaced] {
            assert_eq!(FormerDeviceState::of(state.device_state()), Some(state));
        }
        assert_eq!(FormerDeviceState::of(coven::DeviceState::Active), None);
    }
}
