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
                    Button(action: onBack) {
                        Label("Back", systemImage: "chevron.left")
                    }
                    .buttonStyle(.link)
                    .themeText(.body)
                }
                Spacer(minLength: ThemeSpace.group)
            }
        }
        .padding(.horizontal, ThemeSpace.group)
        .frame(height: 42)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Header") {
        VStack(spacing: 0) {
            FindOnlineHeader(onBack: {})
            Divider()
            FindOnlineHeader(onBack: nil)
        }
        .frame(width: 660)
        .windowBackground()
    }
#endif
