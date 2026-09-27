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
            HStack(spacing: 12) {
                if let onBack {
                    Button(action: onBack) {
                        Label("Back", systemImage: "chevron.left")
                    }
                    .buttonStyle(.link)
                    .themeText(.body)
                }
                Spacer(minLength: 12)
            }
        }
        .padding(.horizontal, 14)
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
