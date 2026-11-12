import BaeKit
import SwiftUI

extension View {
    /// The shared "Error" alert driven by `uiStore.lastError`. Presents while an
    /// error is set, names the concrete fault under core's category line when
    /// core says the fault adds to it, offers a "Copy details" button for the
    /// whole chain, and clears the error on dismiss. Used by every scene that surfaces `UiStore` errors (the main
    /// window and the Storage Manager window own separate alerts on the same
    /// store).
    ///
    /// The fault is in the message, not only behind "Copy details": the category
    /// line alone ("Something went wrong.") names nothing, and an error a reader
    /// has to paste somewhere to identify has not been reported to them.
    func errorAlert(_ uiStore: UiStore) -> some View {
        errorAlert(
            "Error",
            error: Binding(
                get: { uiStore.lastError },
                set: { if $0 == nil { uiStore.clearError() } }
            )
        )
    }

    /// An alert titled `title` for whichever failure `error` holds, shaped like
    /// the shared one: core's line, the concrete fault under it, and "Copy
    /// details" for the whole chain. Dismissing it clears `error`.
    func errorAlert(
        _ title: LocalizedStringKey,
        error: Binding<DisplayError?>
    ) -> some View {
        macAlert(item: error) { error in
            ErrorAlertContent.alert(title: Text(title), error: error)
        }
    }
}

/// What the alert for a failure says: core's line, the concrete fault under it
/// when there is one, and OK with Copy details for the whole chain.
enum ErrorAlertContent {
    static func alert(
        title: Text,
        error: DisplayError
    ) -> (MacAlertWords, [MacAlertButton]) {
        var buttons = [MacAlertButton(Text("OK"))]
        if let detail = error.detail {
            buttons.append(
                MacAlertButton(Text("Copy details")) {
                    SystemActions.copyToPasteboard(detail)
                }
            )
        }
        // Verbatim: both halves are already resolved — core's localized line
        // and its untranslated fault — so this is a join, not prose for a
        // translator.
        let message =
            error.detailSummary.map { "\(error.line)\n\n\($0)" }
            ?? error.line
        return (
            MacAlertWords(title, message: Text(verbatim: message)),
            buttons
        )
    }
}
