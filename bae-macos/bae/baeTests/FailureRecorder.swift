/// Every failure a store told the person rather than stating it on a pane,
/// for a test to read back.
final class FailureRecorder: Sendable {
    private let failures = CallLog<String>()

    func record(_ error: Error) {
        failures.record(String(describing: error))
    }

    /// The failures told to the person, in order.
    func reportedFailures() -> [String] {
        failures.all
    }
}
