import SwiftUI

extension View {
    /// The shared "Error" alert driven by `uiStore.lastError`. Presents while an
    /// error is set, names the concrete fault under core's category line, offers
    /// a "Copy Details" button for the whole chain, and clears the error on
    /// dismiss. Used by every scene that surfaces `UiStore` errors (the main
    /// window and the Storage Manager window own separate alerts on the same
    /// store).
    ///
    /// The fault is in the message, not only behind "Copy Details": the category
    /// line alone ("Something went wrong.") names nothing, and an error a reader
    /// has to paste somewhere to identify has not been reported to them.
    func errorAlert(_ uiStore: UiStore) -> some View {
        alert(
            "Error",
            isPresented: Binding(
                get: { uiStore.lastError != nil },
                set: { if !$0 { uiStore.clearError() } },
            )
        ) {
            if let detail = uiStore.lastError?.detail {
                Button("Copy Details") {
                    SystemActions.copyToPasteboard(detail)
                }
            }
            Button("OK") { uiStore.clearError() }
        } message: {
            if let error = uiStore.lastError {
                if let fault = error.detailSummary {
                    // Verbatim: both halves are already resolved — core's
                    // localized line and its untranslated fault — so this is a
                    // join, not prose for a translator.
                    Text(verbatim: "\(error.line)\n\n\(fault)")
                }
                else {
                    Text(error.line)
                }
            }
        }
    }
}
