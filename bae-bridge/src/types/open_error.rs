use super::{BridgeError, BridgeReplacedBy};

/// Why `init_app` could not open a library, each case one way on. Every
/// case carries the `BridgeError` a surface shows for it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum BridgeOpenError {
    /// This device's copy of the library's data can't be used, and the
    /// library has storage: the app offers Reset from cloud in place of
    /// Retry.
    #[error("{error}")]
    ResetFromCloud { error: BridgeError },
    /// A reset or restore of this device's copy stopped partway; only `by`
    /// finishes it, and the app runs it at once.
    #[error("{error}")]
    ReplacementUnfinished { by: BridgeReplacedBy, error: BridgeError },
    /// This device is removing the library; finishing the removal ends it.
    #[error("{error}")]
    DeletionUnfinished { error: BridgeError },
    /// Any other failure, among them a library that isn't there, a
    /// cancellation and a keychain locked this second.
    #[error("{error}")]
    Failed { error: BridgeError },
}
