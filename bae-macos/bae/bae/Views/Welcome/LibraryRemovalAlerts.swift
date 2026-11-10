import AppKit
import BaeKit
import SwiftUI

/// The alerts of removing a library from where libraries are listed: the
/// confirmation, and the failure of the count before it or of the removal.
@MainActor
enum LibraryRemovalAlerts {
    /// Asks to end `library`, naming it. When the bridge counted changes this
    /// Mac hasn't sent, a warning card leads the message.
    static func confirmation(
        for library: BridgeLibrary,
        losing loss: RemovalLoss
    ) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = LibraryRemovalConfirmation.title(for: library)
        let message = LibraryRemovalConfirmation.message(
            for: library,
            losing: loss
        )
        if LibraryRemovalConfirmation.warnsOfUnsent(loss) {
            alert.informativeText = ""
            alert.accessoryView = AlertAccessory.view(
                VStack(alignment: .leading, spacing: 0) {
                    Notice(
                        title: String(localized: "Unsent changes will be lost"),
                        advice: nil,
                        error: nil,
                        tone: .warning,
                        look: .quiet
                    )
                    Text(verbatim: message).themeText(.body)
                }
            )
        }
        else {
            alert.informativeText = message
        }
        let ending = LibraryRemovalConfirmation.Ending(of: library)
        let action = alert.addButton(withTitle: ending.action)
        action.hasDestructiveAction = true
        let cancel = alert.addButton(withTitle: String(localized: "Cancel"))
        cancel.keyEquivalent = "\u{1b}"
        return alert
    }

    /// Says why the count or the removal of `failed.library` failed, with
    /// Retry first and Cancel.
    static func failure(_ failed: FailedRemoval) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = String(
            localized: "Couldn't remove “\(failed.library.name)”"
        )
        alert.informativeText = failed.error.line
        alert.addButton(withTitle: String(localized: "Retry"))
        let cancel = alert.addButton(withTitle: String(localized: "Cancel"))
        cancel.keyEquivalent = "\u{1b}"
        return alert
    }
}
