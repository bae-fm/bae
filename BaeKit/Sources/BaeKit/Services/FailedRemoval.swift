import Foundation

/// A removal started from where libraries are listed that failed, with what
/// Retry runs again: the count before asking, the removal the person
/// confirmed, or the finishing of one that stopped partway.
public struct FailedRemoval: Sendable {
    public enum Retry: Sendable {
        /// The count failed: count and ask again.
        case ask
        /// The confirmed removal failed: run it again.
        case remove(RemovalLoss)
        /// Finishing a removal that stopped partway failed.
        case finish
    }

    public let library: BridgeLibrary
    public let error: DisplayError
    public let retry: Retry

    public init(library: BridgeLibrary, error: DisplayError, retry: Retry) {
        self.library = library
        self.error = error
        self.retry = retry
    }
}
