import SwiftUI

/// The queue's upcoming read stopped: a centred notice that says so, with
/// Details and Retry, which opens the read again. Shown under the context
/// lane, whose rows past the first window stay placeholders until it does.
public struct UpcomingFailureRow: View {
    private let failure: DisplayError
    private let onRetry: () -> Void

    public init(failure: DisplayError, onRetry: @escaping () -> Void) {
        self.failure = failure
        self.onRetry = onRetry
    }

    public var body: some View {
        Notice(
            title: localizedCoreString("ui.queue.upcoming_failed"),
            advice: nil,
            error: failure,
            look: .centered,
            action: ErrorAction(perform: onRetry)
        )
        .padding(.vertical, ThemeSpace.group)
        .padding(.horizontal, ThemeSpace.inline)
    }
}
