import BaeKit
import SwiftUI

/// The alerts of removing a library from where libraries are listed: the
/// confirmation, and the failure of the count before it or of the removal.
@MainActor
enum LibraryRemovalAlerts {
    /// Asks to end `library`, naming it. When the bridge counted changes this
    /// Mac hasn't sent, a warning leads the message.
    static func confirmation(
        for library: BridgeLibrary,
        losing loss: RemovalLoss
    ) -> (MacAlertWords, [MacAlertButton]) {
        let ending = LibraryRemovalConfirmation.Ending(of: library)
        return (
            MacAlertWords(
                Text(verbatim: LibraryRemovalConfirmation.title(for: library)),
                message: Text(
                    verbatim: LibraryRemovalConfirmation.message(
                        for: library,
                        losing: loss
                    )
                ),
                warning: LibraryRemovalConfirmation.warnsOfUnsent(loss)
                    ? String(localized: "Unsent changes will be lost") : nil
            ),
            [
                // Removing loses nothing (the library stays in the cloud), so
                // it is the plain default button; only a never-synced
                // library's Delete is red.
                MacAlertButton(
                    Text(verbatim: ending.action),
                    role: ending == .delete ? .destructive : .regular
                ),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }

    /// Says why the count or the removal of `failed.library` failed, with
    /// Retry first and Cancel; with OK alone when core says retrying can't
    /// get past it.
    static func failure(_ failed: FailedRemoval) -> (
        MacAlertWords, [MacAlertButton]
    ) {
        let words = MacAlertWords(
            Text("Could not remove \(failed.library.name)"),
            message: Text(verbatim: failed.error.line)
        )
        guard failed.error.offersRetry else {
            return (words, [MacAlertButton(Text("OK"))])
        }
        return (
            words,
            [
                MacAlertButton(Text("Retry")),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }
}
