import Foundation

/// Every failure a store told the person rather than stating it on a pane,
/// for a test to read back.
final class FailureRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var failures: [String] = []

    func record(_ error: Error) {
        lock.recording { failures.append(String(describing: error)) }
    }

    /// The failures told to the person, in order.
    func reportedFailures() -> [String] {
        lock.withLock { failures }
    }
}
