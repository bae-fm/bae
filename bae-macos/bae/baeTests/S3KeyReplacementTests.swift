import BaeKit
import Foundation
import Testing

@testable import bae

/// Replacing the S3 key from the Sync page: the sheet shows each way the
/// call ends, Cancel stops the check, and a call the person cancelled too
/// late still tells the page how it ended.
@Suite("S3 key replacement")
@MainActor
struct S3KeyReplacementTests {
    @Test("a saved key closes the sheet and names the old key")
    func savedKeyClosesTheSheet() async throws {
        let call = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(accessKey: "NEW", secretKey: "secret", sync: call.sync)
        #expect(replacement.sheet == .checking)

        try await call.end(.saved(replaced: "OLD"))
        try await Wait.until { replacement.line != nil }
        #expect(replacement.sheet == nil)
        #expect(replacement.line == .saved(oldKey: "OLD"))
        #expect(call.received == ["NEW"])

        replacement.dismissLine()
        #expect(replacement.line == nil)
    }

    @Test("a key the library already records is saved with no old key")
    func savedKeyWithNoOldKey() async throws {
        let call = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(accessKey: "NEW", secretKey: "secret", sync: call.sync)
        try await call.end(.saved(replaced: nil))
        try await Wait.until { replacement.line != nil }
        #expect(replacement.line == .saved(oldKey: nil))
    }

    @Test("a refused key shows the fields again with why")
    func refusedKeyShowsTheFields() async throws {
        let call = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(accessKey: "NEW", secretKey: "secret", sync: call.sync)
        try await call.end(.refused)
        try await Wait.until { replacement.sheet != .checking }
        #expect(replacement.sheet == .fields(.refused))
        #expect(replacement.line == nil)
    }

    @Test("a failed key shows the fields again with the failure's line")
    func failedKeyShowsTheFields() async throws {
        let call = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(accessKey: "NEW", secretKey: "secret", sync: call.sync)
        try await call.end(
            .failed(error: .Diagnostic(category: .network, detail: "offline"))
        )
        try await Wait.until { replacement.sheet != .checking }
        guard case .fields(.failed(let error)) = replacement.sheet else {
            Issue.record(
                "the sheet shows \(String(describing: replacement.sheet))"
            )
            return
        }
        #expect(error.detail == "offline")
    }

    @Test("Cancel while checking closes the sheet and stops the check")
    func cancelStopsTheCheck() async throws {
        let call = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(accessKey: "NEW", secretKey: "secret", sync: call.sync)
        try await Wait.until { call.cancellation != nil }

        replacement.cancel()
        #expect(replacement.sheet == nil)
        #expect(call.cancellation?.cancelled == true)

        try await call.end(.cancelled)
        try await Wait.until { call.ended }
        #expect(replacement.sheet == nil)
        #expect(replacement.line == nil)
    }

    @Test("a save whose check passed before the cancel still names the old key")
    func saveAfterCancelShowsTheLine() async throws {
        let call = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(accessKey: "NEW", secretKey: "secret", sync: call.sync)
        replacement.cancel()

        try await call.end(.saved(replaced: "OLD"))
        try await Wait.until { replacement.line != nil }
        #expect(replacement.sheet == nil)
        #expect(replacement.line == .saved(oldKey: "OLD"))
    }

    @Test("a failure after the sheet closed shows on the page")
    func failureAfterCancelShowsOnThePage() async throws {
        let call = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(accessKey: "NEW", secretKey: "secret", sync: call.sync)
        replacement.cancel()

        try await call.end(
            .failed(error: .Diagnostic(category: .network, detail: "offline"))
        )
        try await Wait.until { replacement.line != nil }
        #expect(replacement.sheet == nil)
        guard case .failed(let error) = replacement.line else {
            Issue.record(
                "the page shows \(String(describing: replacement.line))"
            )
            return
        }
        #expect(error.detail == "offline")
    }

    @Test("a replaced call's failure shows on the page, not in the sheet")
    func replacedCallFailureShowsOnThePage() async throws {
        let first = HeldKeyCall()
        let second = HeldKeyCall()
        let replacement = S3KeyReplacement(
            makeCancellation: RecordedCancellation.init
        )
        replacement.open()
        replacement.save(
            accessKey: "NEW",
            secretKey: "secret",
            sync: first.sync
        )
        replacement.cancel()
        replacement.open()
        replacement.save(
            accessKey: "NEWER",
            secretKey: "secret",
            sync: second.sync
        )

        try await first.end(
            .failed(error: .Diagnostic(category: .network, detail: "offline"))
        )
        try await Wait.until { replacement.line != nil }
        #expect(replacement.sheet == .checking)
        guard case .failed = replacement.line else {
            Issue.record(
                "the page shows \(String(describing: replacement.line))"
            )
            return
        }
        try await second.end(.cancelled)
    }
}

/// A cancel that records whether it was sent, with no Rust object behind it.
private final class RecordedCancellation: BridgeCancellation,
    @unchecked Sendable
{
    private(set) var cancelled = false

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a recorded cancellation has no handle")
    }

    override func cancel() {
        cancelled = true
        Wait.happened()
    }
}

/// A key call that waits until the test ends it with an outcome.
@MainActor
private final class HeldKeyCall {
    private(set) var received: [String] = []
    private(set) var cancellation: RecordedCancellation?
    private(set) var ended = false
    private var waiting: CheckedContinuation<BridgeAccessKeyOutcome, Never>?

    var sync: Sync {
        Sync(useAccessKey: { [self] accessKey, _, cancellation in
            await self.hold(accessKey, cancellation)
        })
    }

    private func hold(
        _ accessKey: String,
        _ cancellation: BridgeCancellation
    ) async -> BridgeAccessKeyOutcome {
        received.append(accessKey)
        self.cancellation = cancellation as? RecordedCancellation
        let outcome = await withCheckedContinuation {
            waiting = $0
            Wait.happened()
        }
        ended = true
        Wait.happened()
        return outcome
    }

    func end(_ outcome: BridgeAccessKeyOutcome) async throws {
        try await Wait.until { waiting != nil }
        waiting?.resume(returning: outcome)
        waiting = nil
    }
}
