import BaeKit
import SwiftUI

/// The identify bar's trailing controls in a library release's editor: the
/// line of a save that reads audio files first, while it reads them, Cancel
/// while something changed, which keeps its place while hidden, and Save.
struct ReleaseEditControls: View {
    let changed: Bool
    let isBusy: Bool
    /// What a save reads before it writes, while it reads it.
    let reading: String?
    let onCancel: () -> Void
    let onSave: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            if let reading {
                MacSpinner(Text(verbatim: reading))
            }
            MacButton("Cancel", action: onCancel)
                .opacity(changed ? 1 : 0)
                .allowsHitTesting(changed)
                .disabled(isBusy)
            MacButton("Save", kind: .primary, action: onSave)
                .keyboardShortcut(.defaultAction)
                .disabled(isBusy)
        }
    }
}
