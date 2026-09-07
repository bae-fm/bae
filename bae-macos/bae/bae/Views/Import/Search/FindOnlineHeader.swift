import BaeKit
import SwiftUI

/// The Find online title row: the way back and the page's name. What
/// identification and the search have to say about themselves is on their
/// own section headers below, each beside the results it produced.
struct FindOnlineHeader: View {
    /// Leave the pane. `nil` for a surface that owns its own way out — the
    /// re-identify sheet closes rather than going back.
    let onBack: (() -> Void)?

    var body: some View {
        HStack(spacing: 12) {
            if let onBack {
                Button(action: onBack) {
                    Label("Back", systemImage: "chevron.left")
                }
                .buttonStyle(.link)
                .font(.system(size: 13))
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(width: 1, height: 14)
            }
            Text("Find online")
                .font(.system(size: 13, weight: .semibold))
            Spacer(minLength: 12)
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
