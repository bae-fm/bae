import AppKit

/// The alert Settings → Storage asks before it deletes what a reset left.
@MainActor
enum StorageAlerts {
    /// Asks to delete the data from before the reset, which is `size` on this
    /// Mac. The library is not affected, and the data can't be recovered.
    static func deleteBeforeReset(size: String) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = String(
            localized: "Delete data from before the reset?"
        )
        alert.informativeText = String(
            localized:
                "\(size) will be removed from this Mac. It cannot be recovered. Your library is not affected."
        )
        let delete = alert.addButton(withTitle: String(localized: "Delete"))
        delete.hasDestructiveAction = true
        let cancel = alert.addButton(withTitle: String(localized: "Cancel"))
        cancel.keyEquivalent = "\u{1b}"
        return alert
    }

    /// Show `alert` over the window the person is in, or on its own when
    /// there is none, and answer whether its first button was chosen.
    static func confirms(_ alert: NSAlert) async -> Bool {
        let response: NSApplication.ModalResponse
        if let window = NSApp.keyWindow {
            response = await alert.beginSheetModal(for: window)
        }
        else {
            response = alert.runModal()
        }
        return response == .alertFirstButtonReturn
    }
}
