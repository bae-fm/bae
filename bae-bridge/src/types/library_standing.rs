use super::BridgeSyncShown;

/// How a library stands with its storage: as its open session knows it, or
/// as this device knew it when it closed.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeLibraryStanding {
    pub state: BridgeStandingState,
    /// When sync last finished, if it ever did.
    pub last_synced: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeStandingState {
    /// This device holds writes or files it hasn't uploaded, whatever state
    /// sync was in.
    UnsentChanges,
    /// Nothing waits to upload: sync's state, worded as the Sync page words
    /// it.
    Sync { shown: BridgeSyncShown },
}

impl BridgeLibraryStanding {
    pub(crate) fn from_core(standing: bae_library::LibraryStanding) -> Self {
        let state = match standing.state {
            bae_library::StandingState::UnsentChanges => BridgeStandingState::UnsentChanges,
            bae_library::StandingState::Sync(shown) => {
                BridgeStandingState::Sync { shown: BridgeSyncShown::from_core(shown) }
            }
        };
        Self { state, last_synced: standing.last_synced }
    }
}
