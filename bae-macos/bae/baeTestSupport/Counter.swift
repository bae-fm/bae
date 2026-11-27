import BaeKit

/// A count that a test's closures raise from any thread and the test reads
/// and waits for.
final class Counter: Sendable {
    private let raised = Recorded(0)

    var count: Int { raised.value }

    /// Raise the count by one and return what it became.
    @discardableResult
    func increment() -> Int {
        raised.change {
            $0 += 1
            return $0
        }
    }

    func decrement() { raised.change { $0 -= 1 } }

    /// Raise the count to `value` if it is below it.
    func raise(to value: Int) { raised.change { $0 = max($0, value) } }
}
