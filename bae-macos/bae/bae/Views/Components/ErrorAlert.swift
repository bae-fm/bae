import SwiftUI

extension View {
    /// The shared "Error" alert driven by `uiStore.lastError`. Presents while an
    /// error is set, offers a "Copy Details" button when the error carries
    /// opaque diagnostic detail, and clears the error on dismiss. Used by every
    /// scene that surfaces `UiStore` errors (the main window and the Storage
    /// Manager window own separate alerts on the same store).
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
                Text(error.line)
            }
        }
    }
}
