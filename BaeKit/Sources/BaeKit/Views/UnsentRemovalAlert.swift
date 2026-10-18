import SwiftUI

extension View {
    /// Ask about the unsent changes that stopped a library's removal: how many
    /// would be lost, then Remove Anyway or Cancel. `consequence` says, in the
    /// platform's words, that removing the library from this device loses
    /// them.
    public func unsentRemovalAlert(
        _ removals: LibraryRemovals,
        consequence: String
    ) -> some View {
        alert(
            "Unsent changes will be lost",
            // Both buttons answer the question, which ends it.
            isPresented: Binding(
                get: { removals.unsent != nil },
                set: { _ in }
            ),
            presenting: removals.unsent
        ) { _ in
            Button("Remove Anyway", role: .destructive) {
                removals.discardUnsent()
            }
            Button("Cancel", role: .cancel) { removals.keepUnsent() }
        } message: { unsent in
            Text((unsent.edits.lines + [consequence]).joined(separator: " "))
        }
    }
}
