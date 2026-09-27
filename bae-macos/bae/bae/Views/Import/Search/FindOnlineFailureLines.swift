import BaeKit
import SwiftUI

/// Every automatic lookup failed, so the reasons take the place of the results.
/// `onRetry` is `nil` when a ledger above already offers Retry in the failed
/// cell.
struct FindOnlineFailureLines: View {
    let failures: [BridgeIdentifyFailure]
    let onRetry: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(failures, id: \.badgeLine) { failure in
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(Theme.warning)
                    Text(failure.badgeLine)
                    Spacer(minLength: 0)
                }
            }
            if let onRetry {
                Button("Retry", action: onRetry)
                    .buttonStyle(.link)
            }
        }
        .font(.system(size: 12.5))
        .padding(.horizontal, 18)
        .padding(.vertical, 22)
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }
}
