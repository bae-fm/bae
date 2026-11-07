//! How a device lost access to the library.

/// How a device lost access to the library, as coven records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormerDeviceState {
    Removed,
    Replaced,
}

impl FormerDeviceState {
    /// The state coven's `coven_devices.state` text names, or `None` for
    /// `active` and for text coven doesn't write.
    pub fn of_state(text: &str) -> Option<Self> {
        match text {
            "removed" => Some(Self::Removed),
            "replaced" => Some(Self::Replaced),
            _ => None,
        }
    }

    /// The text of this state, as `of_state` reads it.
    pub fn state_text(self) -> &'static str {
        match self {
            Self::Removed => "removed",
            Self::Replaced => "replaced",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FormerDeviceState;

    /// Every state reads back from its text; `active` and anything else is
    /// no former state.
    #[test]
    fn a_state_reads_back_from_its_text() {
        for state in [FormerDeviceState::Removed, FormerDeviceState::Replaced] {
            assert_eq!(FormerDeviceState::of_state(state.state_text()), Some(state));
        }
        assert_eq!(FormerDeviceState::of_state("active"), None);
        assert_eq!(FormerDeviceState::of_state("gone"), None);
    }
}
