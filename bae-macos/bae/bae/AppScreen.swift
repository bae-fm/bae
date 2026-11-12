import BaeKit
import Foundation

/// What the main window shows: what it waits on, a library's recovery after
/// it couldn't open or sync, or the window's home.
enum AppScreen {
    case loading(LoadingWork)
    /// Nothing else is up: the open library when one is open, else the
    /// welcome. Whether a library is open decides which, so the window can't
    /// show the welcome over an open library or ask for the library without
    /// one.
    case home
    /// Opening found a damaged database or another copy of this device; the
    /// person can reset the device for this library.
    case resetRequired(libraryId: String)
    /// Another of the person's devices replaced their keys; the library is
    /// closed and the person restores it with their current restore code.
    case restoreRequired(libraryId: String)
    /// A reset of this device's copy stopped partway, and finishing it
    /// failed; finishing tries again.
    case resetUnfinished(libraryId: String)
    /// A restore of this device's copy stopped partway. Finishing it from
    /// what core kept begins at once; the person's current restore code
    /// finishes it too.
    case restoreUnfinished(libraryId: String)
    case keychainLocked(libraryId: String)

    /// The screen that finishes a reset or restore of `libraryId` that
    /// stopped partway, by what `by` names.
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

    /// What the loading screen waits on, which it names while no step of it
    /// runs.
    enum LoadingWork {
        /// Reading which libraries this Mac holds, at launch.
        case findingLibraries
        /// A setup call before any library is open: the debug app's dev
        /// reset.
        case settingUp
        case opening
        case closing
        /// Resetting this device's copy of a library.
        case resetting

        /// What the screen says it waits on.
        var title: LocalizedStringResource {
            switch self {
            case .findingLibraries:
                LocalizedStringResource(
                    "Finding your libraries…",
                    comment:
                        "Loading screen at launch, while bae reads which libraries this Mac holds."
                )
            case .settingUp:
                LocalizedStringResource(
                    "Setting up the library…",
                    comment:
                        "Loading screen while a setup call runs, before it reports its first step."
                )
            case .opening:
                LocalizedStringResource(
                    "Opening the library…",
                    comment:
                        "Loading screen while a library opens and no step of the open is running."
                )
            case .closing:
                LocalizedStringResource(
                    "Closing the library…",
                    comment:
                        "Loading screen while the open library closes, before the close reports its first step."
                )
            case .resetting:
                LocalizedStringResource(
                    "Resetting from cloud…",
                    comment:
                        "Loading screen while this Mac's copy of a library is reset, before the reset reports where it stands."
                )
            }
        }
    }

    /// The screen a failed open of `libraryId` lands on: the call that
    /// recovers it, when the failure is one the person recovers from here;
    /// else home, which shows the library still open with the failure over
    /// it, or the welcome when none is.
    static func afterFailedOpen(libraryId: String, from error: Error) -> Self {
        recovering(libraryId: libraryId, from: error) ?? .home
    }
}
