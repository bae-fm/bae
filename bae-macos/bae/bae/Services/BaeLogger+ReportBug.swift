import BaeKit

extension BaeLogger {
    /// Logs and reports a bug, and gives its caller the diagnostic to present.
    @discardableResult
    func reportBug(
        operation: String,
        detail: String,
        to reporter: any BridgeBugReporter
    ) -> BridgeError {
        error("\(operation) failed: \(detail)")
        reporter.report(operation: operation, detail: detail)
        return .Diagnostic(
            category: .internal,
            detail: "\(operation): \(detail)"
        )
    }
}
