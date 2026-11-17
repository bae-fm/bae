use super::{BridgeError, BridgeReplacedBy};

/// Why `init_app` could not open a library. Each case names the one thing
/// the app offers next, and carries the `BridgeError` a surface shows for
/// it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum BridgeOpenError {
    /// bae can't use this device's copy of the library's data, and the
    /// library has storage: the app offers Reset from cloud in place of
    /// Retry.
    #[error("{error}")]
    ResetFromCloud { error: BridgeError },
    /// A reset or restore of this device's copy stopped partway; only `by`
    /// finishes it, and the app runs it at once.
    #[error("{error}")]
    ReplacementUnfinished { by: BridgeReplacedBy, error: BridgeError },
    /// This device is removing the library, and the app offers to finish the
    /// removal.
    #[error("{error}")]
    DeletionUnfinished { error: BridgeError },
    /// The OS keychain refused the library's key for now, as it does while
    /// the person's session is locked: the app opens the library again once
    /// the person unlocks the keychain.
    #[error("{error}")]
    KeychainLocked { error: BridgeError },
    /// Any other failure, among them a library that isn't there and a
    /// cancellation.
    #[error("{error}")]
    Failed { error: BridgeError },
}
