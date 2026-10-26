import SwiftUI

/// The pane for a selected item whose read has not been delivered yet.
///
/// Empty rather than a placeholder, and empty rather than a spinner: the read
/// almost always lands within a frame or two, and a spinner that appears and
/// leaves that fast is a flash of its own. One shows only if the wait outlasts
/// `spinnerDelay`, which is the case worth reporting — a read that is
/// genuinely slow.
struct PendingReadPane<Selection: Hashable>: View {
    /// Restarts the wait when the selection moves to another item while one
    /// is still pending.
    let selection: Selection

    private static var spinnerDelay: Duration { .milliseconds(150) }

    @State
    private var waitedLongEnough = false

    var body: some View {
        VStack {
            if waitedLongEnough {
                ProgressView().controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: selection) {
            waitedLongEnough = false
            try? await Task.sleep(for: Self.spinnerDelay)
            guard !Task.isCancelled else { return }
            waitedLongEnough = true
        }
    }
}
