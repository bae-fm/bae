import BaeKit
import SwiftUI

/// Confirmation for moving a local release to cloud storage.
struct MoveToCloudConfirmSheet: View {
    let onConfirm: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Move to cloud")
                .themeText(.heading)
            MacSheetFooter {
                MacButton("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                MacButton("Move to cloud", kind: .primary) { onConfirm() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.top, ThemeSpace.group)
        .padding(.bottom, ThemeSpace.edge)
        .background(Theme.surfaceElevated)
    }
}
