import BaeKit
import SwiftUI

/// A way to answer the folder other than picking a row: the button, and what
/// it does beneath it. `off` while bae imports the folder.
struct MacAnswerAction: View {
    let title: String
    let subtitle: String
    var off = false
    let action: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            Button(title, action: action)
                .buttonStyle(.accentBordered)
                .controlSize(.small)
                .disabled(off)
            Text(subtitle)
                .themeText(.detail)
                .foregroundStyle(Theme.textTertiary)
        }
    }
}
