import BaeKit
import Testing

@testable import bae

@MainActor
private func waitUntil(_ predicate: @MainActor () -> Bool) async {
    for _ in 0..<100 {
        if predicate() {
            return
        }
        await Task.yield()
    }
    #expect(predicate())
}

private actor ProjectionProbe {
    private var continuations: [CheckedContinuation<Int, Never>] = []
    private var waiters: [(Int, CheckedContinuation<Void, Never>)] = []

    func query() async -> Int {
        await withCheckedContinuation { continuation in
            continuations.append(continuation)
            resumeSatisfiedWaiters()
        }
    }

    func waitUntilQueryCount(_ count: Int) async {
        guard continuations.count < count else {
            return
        }
        await withCheckedContinuation { continuation in
            waiters.append((count, continuation))
        }
    }

    func resumeNext(_ value: Int) {
        let continuation = continuations.removeFirst()
        continuation.resume(returning: value)
    }

    private func resumeSatisfiedWaiters() {
        var remaining: [(Int, CheckedContinuation<Void, Never>)] = []
        for waiter in waiters {
            if continuations.count >= waiter.0 {
                waiter.1.resume()
            }
            else {
                remaining.append(waiter)
            }
        }
        waiters = remaining
    }
}

/// Counts cancellations of the probe's query. A cancellation handler runs
/// outside the projection's actor, so the count lives on a reference the test
/// and the handler share.
private final class CancelCount: @unchecked Sendable {
    var value = 0
}

@Suite("Projection")
struct ProjectionTests {
    @MainActor
    @Test("matching invalidation queries and applies the returned value")
    func matchingInvalidationAppliesValue() async {
        var applied: [Int] = []
        let projection = Projection<Int>(
            domain: .config,
            query: { _ in 7 },
            apply: { applied.append($0) },
            onError: { _ in }
        )

        projection.invalidate(for: .config)
        await waitUntil { applied == [7] }

        #expect(applied == [7])
        #expect(projection.generation == 1)
    }

    @MainActor
    @Test("nonmatching invalidation does not query")
    func nonmatchingInvalidationDoesNotQuery() async {
        let projection = Projection<Int>(
            domain: .config,
            query: { _ in 7 },
            apply: { _ in },
            onError: { _ in }
        )

        projection.invalidate(for: .queue)

        #expect(projection.generation == 0)
    }

    /// An invalidation arriving while a query is running does not cancel it —
    /// it queues one re-read behind it.
    ///
    /// A projection whose query is cancellable (every `async` bridge call is)
    /// and whose domain is invalidated faster than the query completes is
    /// otherwise starved: each invalidation kills the read before it lands and
    /// the store never moves. That is what import progress does to the triage
    /// queue — one invalidation per percent, against a read of every candidate.
    @MainActor
    @Test("an invalidation mid-query queues a re-read instead of cancelling")
    func midQueryInvalidationQueuesAReread() async {
        let probe = ProjectionProbe()
        let cancels = CancelCount()
        var applied: [Int] = []
        let projection = Projection<Int>(
            domain: .config,
            query: { _ in
                await withTaskCancellationHandler {
                    await probe.query()
                } onCancel: {
                    cancels.value += 1
                }
            },
            apply: { applied.append($0) },
            onError: { _ in }
        )

        projection.invalidate(for: .config)
        await probe.waitUntilQueryCount(1)
        // Three more while the first is still out. They collapse into one
        // re-read, not three.
        projection.invalidate(for: .config)
        projection.invalidate(for: .config)
        projection.invalidate(for: .config)
        #expect(cancels.value == 0)

        await probe.resumeNext(1)
        await waitUntil { applied == [1] }

        // The queued re-read only goes out once the first one is back, so this
        // waits on the probe holding one query again, not two at once.
        await probe.waitUntilQueryCount(1)
        await probe.resumeNext(2)
        await waitUntil { applied == [1, 2] }

        #expect(cancels.value == 0)
        #expect(applied == [1, 2])
        #expect(projection.generation == 2)
    }
}

@Suite("ProjectionRegistry")
struct ProjectionRegistryTests {
    @MainActor
    @Test("registry routes matching invalidations and unregisters tokens")
    func routesAndUnregisters() async {
        let registry = ProjectionRegistry()
        var invalidations: [BridgeInvalidation] = []
        var registration: ProjectionRegistration? = registry.register(
            domains: [.release],
            invalidate: { invalidations.append($0) }
        )

        registry.invalidate(.release(releaseId: "release-1"))
        registry.invalidate(.config)
        _ = registration
        #expect(invalidations == [.release(releaseId: "release-1")])

        registration = nil
        registry.invalidate(.release(releaseId: "release-2"))
        #expect(invalidations == [.release(releaseId: "release-1")])
    }
}
