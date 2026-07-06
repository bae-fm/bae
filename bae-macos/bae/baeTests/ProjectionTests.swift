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

    @MainActor
    @Test("newer invalidation wins over an older in-flight query")
    func newerInvalidationWins() async {
        let probe = ProjectionProbe()
        var applied: [Int] = []
        let projection = Projection<Int>(
            domain: .config,
            query: { _ in await probe.query() },
            apply: { applied.append($0) },
            onError: { _ in }
        )

        projection.invalidate(for: .config)
        await probe.waitUntilQueryCount(1)
        projection.invalidate(for: .config)
        await probe.waitUntilQueryCount(2)
        await probe.resumeNext(1)
        await probe.resumeNext(2)
        await waitUntil { applied == [2] }

        #expect(applied == [2])
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
