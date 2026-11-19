import BaeKit
import SwiftUI

/// The identify bar's trailing controls in a library release's editor:
/// Cancel while something changed, which keeps its place while hidden, and
/// Save.
struct ReleaseEditControls: View {
    let changed: Bool
    let isBusy: Bool
    let onCancel: () -> Void
    let onSave: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Button("Cancel", action: onCancel)
                .opacity(changed ? 1 : 0)
                .allowsHitTesting(changed)
                .disabled(isBusy)
            MacButton("Save", kind: .primary, action: onSave)
                .keyboardShortcut(.defaultAction)
                .disabled(isBusy)
        }
    }
}
