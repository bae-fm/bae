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
                    MacButton(kind: .borderless, action: onBack) {
                        Label("Back", systemImage: "chevron.backward")
                    }
                    .themeText(.body)
                }
                Spacer(minLength: ThemeSpace.group)
            }
        }
        .padding(.horizontal, ThemeSpace.group)
        .frame(height: 42)
    }
}
