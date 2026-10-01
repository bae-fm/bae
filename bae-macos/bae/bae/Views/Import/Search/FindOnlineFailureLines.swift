import BaeKit
import SwiftUI

/// The failures no cell of the band shows, each with its reason, and the
/// retry no cell offers for them.
struct FindOnlineFailureLines: View {
    let failures: [BridgeIdentifyFailure]
    let onRetry: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            ForEach(failures, id: \.badgeLine) { failure in
                HStack(alignment: .top, spacing: ThemeSpace.related) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(Theme.warning)
                    Text(failure.badgeLine)
                    Spacer(minLength: 0)
                }
            }
            Button("Retry", action: onRetry)
                .buttonStyle(.link)
        }
        .themeText(.body)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.section)
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }
}
