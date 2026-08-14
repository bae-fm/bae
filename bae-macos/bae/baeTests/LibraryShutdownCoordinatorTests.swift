@testable import bae
import Testing

@MainActor
@Suite("Library shutdown coordinator")
struct LibraryShutdownCoordinatorTests {
    @Test("a failed shutdown remains failed for every waiter")
    func failedShutdownIsShared() async {
        let coordinator = LibraryShutdownCoordinator<TestOwner>()
        let owner = TestOwner()
        let calls = CallCount()

        let first = coordinator.begin(for: owner) {
            await calls.increment()
            throw TestFailure()
        }
        let second = coordinator.begin(for: owner) {
            Issue.record("the shared shutdown operation ran twice")
        }

        #expect(first.started)
        #expect(!second.started)
        guard case .failed = await first.task.value else {
            Issue.record("the shutdown failure was hidden")
            return
        }
        guard case .failed = await second.task.value else {
            Issue.record("the second waiter did not receive the failure")
            return
        }
        #expect(await calls.value == 1)
    }

    @Test("the coordinator records which library is shutting down")
    func tracksPendingOwner() async {
        let coordinator = LibraryShutdownCoordinator<TestOwner>()
        let firstOwner = TestOwner()
        let secondOwner = TestOwner()
        let gate = AsyncGate()

        let attempt = coordinator.begin(for: firstOwner) {
            await gate.wait()
        }

        #expect(coordinator.hasPendingShutdown(for: firstOwner))
        #expect(!coordinator.hasPendingShutdown(for: secondOwner))
        await gate.open()
        guard case .completed = await attempt.task.value else {
            Issue.record("the gated shutdown did not complete")
            return
        }
        coordinator.finish(for: firstOwner)
        #expect(!coordinator.hasPendingShutdown(for: firstOwner))
    }
}

private final class TestOwner: @unchecked Sendable {}

private struct TestFailure: Error {}

private actor CallCount {
    private(set) var value = 0

    func increment() {
        value += 1
    }
}

private actor AsyncGate {
    private var isOpen = false
    private var continuation: CheckedContinuation<Void, Never>?

    func wait() async {
        if isOpen { return }
        await withCheckedContinuation { continuation in
            self.continuation = continuation
        }
    }

    func open() {
        isOpen = true
        continuation?.resume()
        continuation = nil
    }
}
