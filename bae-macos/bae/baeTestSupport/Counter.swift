import Foundation

/// A count that a test's closures raise from any thread and the test reads.
final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var raised = 0

    var count: Int { lock.withLock { raised } }

    func increment() { lock.withLock { raised += 1 } }
}
