import BaeKit

/// What the main window shows: a library opening, the welcome, a library's
/// recovery after it couldn't open or sync, or the open library.
enum AppScreen {
    case loading
    case welcome
    /// Opening found a damaged database or another copy of this device; the
    /// person can reset the device for this library.
    case resetRequired(libraryId: String)
    /// Another of the person's devices replaced their keys; the library is
    /// closed and the person restores it with their current restore code.
    case restoreRequired(libraryId: String)
    /// A reset of this device's copy stopped partway; resetting again
    /// finishes it.
    case resetUnfinished(libraryId: String)
    /// A restore of this device's copy stopped partway; restoring again with
    /// the person's current restore code finishes it.
    case restoreUnfinished(libraryId: String)
    case keychainLocked(libraryId: String)
    case library

    /// The screen that finishes a reset or restore of `libraryId` that
    /// stopped partway: the one call `by` names.
    static func finishing(libraryId: String, by: BridgeReplacedBy) -> Self {
        switch by {
        case .reset: .resetUnfinished(libraryId: libraryId)
        case .restore: .restoreUnfinished(libraryId: libraryId)
        }
    }

    /// The screen a failed open of `libraryId` lands on when its failure is
    /// one the person recovers from here: resetting the device, or finishing
    /// the reset or restore that stopped partway. `nil` for any other
    /// failure.
    static func recovering(libraryId: String, from error: Error) -> Self? {
        guard case BridgeError.Diagnostic(let category, _) = error else {
            return nil
        }
        switch category {
        case .deviceResetRequired:
            return .resetRequired(libraryId: libraryId)
        case .replacementUnfinished(let by):
            return .finishing(libraryId: libraryId, by: by)
        default:
            return nil
        }
    }
}
