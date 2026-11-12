import BaeKit
import SwiftUI

/// The close control shared by docked panes and expanded detail cards.
struct MacCloseButton: View {
    let onClose: () -> Void

    var body: some View {
        Button {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                onClose()
            }
        } label: {
            Image(systemName: "xmark")
                .themeIcon(.medium)
                .foregroundStyle(.secondary)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .background(Theme.hover, in: Circle())
        }
        .buttonStyle(.plain)
        .help("Close")
        .accessibilityLabel("Close")
    }
}
