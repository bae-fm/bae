import BaeKit
import Foundation

@testable import bae

/// A subscription that answers `value` once and then waits until it is
/// cancelled, for shots and tests, which run without the bridge.
final class FixedSubscription<Value: Sendable>: PulledSubscription,
    @unchecked Sendable
{
    private let lock = NSLock()
    private var answered = false
    private let value: Value
    private var waiter: CheckedContinuation<Value, any Error>?

    init(value: Value) { self.value = value }

    func next() async throws -> Value {
        let first = lock.withLock {
            defer { answered = true }
            return !answered
        }
        if first { return value }
        return try await withCheckedThrowingContinuation { continuation in
            lock.withLock { waiter = continuation }
        }
    }

    func cancel() async throws {
        let waiter = lock.withLock {
            defer { self.waiter = nil }
            return self.waiter
        }
        waiter?.resume(throwing: BridgeError.Cancelled)
    }
}

/// An apply of a catalog release that waits on `step` while a line reads
/// it, for shots and tests, which run without the bridge.
final class FixedCatalogApply: CatalogApplySource {
    let step: BridgeCatalogApplyStep?

    init(step: BridgeCatalogApplyStep?) { self.step = step }

    var bridgeApply: BridgeCatalogApply? { nil }

    func stepValues() -> PulledValues<BridgeCatalogApplyStep?> {
        PulledValues(FixedSubscription(value: step))
    }
}
