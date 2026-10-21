import SwiftUI

/// The queue's upcoming read stopped: why, with its details, and Retry,
/// which opens the read again. Shown under the context lane, whose rows past
/// the first window stay placeholders until it does.
public struct UpcomingFailureRow: View {
    private let failure: DisplayError
    private let onRetry: () -> Void

    public init(failure: DisplayError, onRetry: @escaping () -> Void) {
        self.failure = failure
        self.onRetry = onRetry
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ErrorDetailDisclosure(error: failure)
            Button("Retry", action: onRetry)
        }
        .padding(ThemeSpace.group)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
