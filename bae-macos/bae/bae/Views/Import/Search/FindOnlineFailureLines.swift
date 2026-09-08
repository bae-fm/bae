import BaeKit
import SwiftUI

/// Every automatic lookup failed, so the reasons take the place of the results.
///
/// The way to ask again belongs wherever the person is looking. With a ledger
/// above these lines that is the Retry in the cell that failed, and `onRetry`
/// is `nil` here. Without one — a folder that carried nothing to lay out, or a
/// verdict stored before its signals were — these lines are the whole pane, and
/// this is the only way back.
struct FindOnlineFailureLines: View {
    let failures: [BridgeIdentifyFailure]
    let onRetry: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(failures, id: \.badgeLine) { failure in
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
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
