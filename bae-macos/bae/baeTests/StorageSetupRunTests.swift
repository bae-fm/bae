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

@MainActor
@Suite("A sheet's storage setup and its Cancel")
struct StorageSetupRunTests {
    /// A cancel before the point of no return shows under way until the
    /// setup's call ends cancelled, which coven answers once it has undone
    /// what setup wrote; only then does the run end.
    @Test("a cancel shows under way until the call ends")
    func cancelShowsUnderWayUntilTheCallEnds() async throws {
        let call = HeldCall()
        let run = StorageSetupRun()
        run.start(
            StorageSetupFollowing(
                staging: BridgeStorageSetupProgress(
                    step: .connecting,
                    key: "core.library.storage_setup_step.connecting",
                    done: 0,
                    total: 1,
                    fraction: nil
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

        call.fail(BridgeError.Cancelled)
        try await Wait.until { !run.isRunning }
        #expect(!run.isCancelling)
    }

    /// Past its point of no return the setup finishes: Cancel isn't
    /// offered, and the sheet stays to show it.
    @Test("a setup past its point of no return offers no Cancel")
    func finishingOffersNoCancel() async throws {
        let call = HeldCall()
        let run = StorageSetupRun()
        run.start(
            StorageSetupFollowing(
                staging: BridgeStorageSetupProgress(
                    step: .creatingStore,
                    key: "core.library.storage_setup_step.creating_store",
                    done: 0,
                    total: 1,
                    fraction: nil
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
