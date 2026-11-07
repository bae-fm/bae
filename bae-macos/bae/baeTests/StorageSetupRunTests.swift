import BaeKit
import Foundation
import Testing

@testable import bae

/// A setup call the test answers in place of core.
@MainActor
@Observable
private final class HeldCall {
    private var caller: CheckedContinuation<BridgeStorageSetup, any Error>?

    func call() async throws -> BridgeStorageSetup {
        try await withCheckedThrowingContinuation { caller = $0 }
    }

    var waiting: Bool { caller != nil }

    func fail(_ error: any Error) {
        caller?.resume(throwing: error)
        caller = nil
    }
}

/// Whether the sheet closed for a cancelled setup.
@MainActor
@Observable
private final class Closing {
    var closed = false
}

@MainActor
@Suite("A sheet's storage setup and its Cancel")
struct StorageSetupRunTests {
    /// A cancel before the point of no return shows under way until the
    /// setup's call ends cancelled, which coven answers once it has undone
    /// what setup wrote; only then does the sheet close.
    @Test("a cancel shows under way until the call ends, then closes")
    func cancelShowsUnderWayUntilTheCallEnds() async throws {
        let call = HeldCall()
        let closing = Closing()
        let run = StorageSetupRun(onCancelled: { closing.closed = true })
        run.start(
            StorageSetupFollowing(
                staging: BridgeStorageSetupProgress(
                    step: .connecting,
                    done: 0,
                    total: 1
                ),
                finishing: false
            ),
            operation: { _ in try await call.call() },
            onConnected: { _ in Issue.record("a cancelled setup connected") },
            onFailed: { _ in Issue.record("a cancelled setup failed") }
        )
        #expect(run.cancelOffered)
        try await Wait.until { call.waiting }

        run.cancel(Sync(cancelStorageSetup: { .undoing }))
        guard case .cancelling = run.state else {
            Issue.record("Cancel left the run \(run.state)")
            return
        }
        #expect(!run.cancelOffered)
        #expect(!closing.closed)

        call.fail(BridgeError.Cancelled)
        try await Wait.until { closing.closed }
        #expect(!run.isRunning)
    }

    /// Past its point of no return the setup finishes: Cancel isn't
    /// offered, and the sheet stays to show it.
    @Test("a setup past its point of no return offers no Cancel")
    func finishingOffersNoCancel() async throws {
        let call = HeldCall()
        let run = StorageSetupRun(onCancelled: {
            Issue.record("a finishing setup closed as cancelled")
        })
        run.start(
            StorageSetupFollowing(
                staging: BridgeStorageSetupProgress(
                    step: .creatingStore,
                    done: 0,
                    total: 1
                ),
                finishing: true
            ),
            operation: { _ in try await call.call() },
            onConnected: { _ in },
            onFailed: { _ in }
        )
        #expect(run.isRunning)
        #expect(!run.cancelOffered)
        try await Wait.until { call.waiting }
        call.fail(CancellationError())
        try await Wait.until { !run.isRunning }
    }
}
