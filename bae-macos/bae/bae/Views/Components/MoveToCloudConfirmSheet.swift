import BaeKit
import SwiftUI

/// Confirmation for moving a local release to cloud storage.
struct MoveToCloudConfirmSheet: View {
    let onConfirm: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text("Move to cloud")
                .themeText(.heading)
            MacSheetFooter {
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Move to cloud") { onConfirm() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }
}

#if DEBUG
    #Preview("Move to cloud") {
        MoveToCloudConfirmSheet(onConfirm: {}, onCancel: {})
            .frame(width: 420)
    }
#endif
