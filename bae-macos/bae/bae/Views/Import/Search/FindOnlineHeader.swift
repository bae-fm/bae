import BaeKit
import SwiftUI

/// The Identify title row: the way back, and the page's name centered.
struct FindOnlineHeader: View {
    /// Leave the pane; `nil` for a surface with its own way out.
    let onBack: (() -> Void)?

    var body: some View {
        ZStack {
            Text("Identify")
                .themeText(.strong)
            HStack(spacing: ThemeSpace.group) {
                if let onBack {
                    // A plain accent button, as the board draws it: the
                    // chevron and the word with no box round them.
                    MacButton(kind: .plain, action: onBack) {
                        HStack(spacing: ThemeSpace.inline) {
                            Image(systemName: "chevron.backward")
                                .themeIcon(.small)
                                // The board's 12 pt chevron box.
                                .frame(width: 12)
                                .accessibilityHidden(true)
                            Text("Back")
                        }
                    }
                    .accent()
                    .themeText(.body)
                }
                Spacer(minLength: ThemeSpace.group)
            }
        }
        .padding(.horizontal, ThemeSpace.group)
        .frame(height: 42)
    }
}
