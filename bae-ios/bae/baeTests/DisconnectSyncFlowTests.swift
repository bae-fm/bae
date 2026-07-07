import Foundation
import Testing

@testable import bae

/// Covers `DisconnectSyncFlow`'s confirmation and execution logic with stubbed
/// bridge/keychain closures — no live core. The load-bearing invariants are the
/// warning-append composition, that the confirmation opens even when the
/// at-risk check fails, and that the restore code is deleted only after a
/// successful disconnect.
@MainActor
@Suite("DisconnectSyncFlow")
struct DisconnectSyncFlowTests {
    /// Thread-safe call recorder for the injected closures. `@unchecked
    /// Sendable` because the `disconnect` closure is invoked off the main actor
    /// by `DetachedWork.run`.
    private final class Recorder: @unchecked Sendable {
        private let lock = NSLock()
        private var _warningCalls = 0
        private var _disconnectCalls = 0
        private var _deleteCalls = 0

        func recordWarning() -> Int {
            lock.withLock {
                defer { _warningCalls += 1 }
                return _warningCalls
            }
        }
        func recordDisconnect() { lock.withLock { _disconnectCalls += 1 } }
        func recordDelete() { lock.withLock { _deleteCalls += 1 } }

        var disconnectCalls: Int { lock.withLock { _disconnectCalls } }
        var deleteCalls: Int { lock.withLock { _deleteCalls } }
    }

    private func makeFlow(
        warningMessage: @escaping @Sendable () async throws -> String? = {
            nil
        },
        disconnect: @escaping @Sendable () throws -> Void = {},
        deleteRestoreCode: @escaping () -> Void = {}
    ) -> DisconnectSyncFlow {
        DisconnectSyncFlow(
            warningMessage: warningMessage,
            disconnect: disconnect,
            deleteRestoreCode: deleteRestoreCode
        )
    }

    private func waitUntil(_ condition: @MainActor () -> Bool) async {
        for _ in 0..<10_000 {
            if condition() { return }
            await Task.yield()
        }
        Issue.record("condition never became true")
    }

    @Test("prompt appends the at-risk warning after a single space")
    func promptAppendsWarning() async {
        let warning = "2 releases are only in the cloud."
        let flow = makeFlow(warningMessage: { warning })
        let base = flow.message

        flow.promptDisconnect()
        await waitUntil { flow.showConfirm }

        #expect(flow.extraWarning == warning)
        #expect(flow.message == "\(base) \(warning)")
    }

    @Test("prompt with no at-risk releases shows the base copy only")
    func promptWithoutWarning() async {
        let flow = makeFlow(warningMessage: { nil })
        let base = flow.message

        flow.promptDisconnect()
        await waitUntil { flow.showConfirm }

        #expect(flow.extraWarning == nil)
        #expect(flow.message == base)
    }

    @Test("a failed at-risk check still opens the confirmation, with an error")
    func promptWarningFailureStillOpens() async {
        let flow = makeFlow(warningMessage: { throw StubError.notImplemented })

        flow.promptDisconnect()
        await waitUntil { flow.showConfirm }

        #expect(flow.extraWarning == nil)
        #expect(flow.error != nil)
    }

    @Test("a successful disconnect deletes the restore code exactly once")
    func confirmSuccessDeletesRestoreCode() async {
        let recorder = Recorder()
        let flow = makeFlow(
            disconnect: { recorder.recordDisconnect() },
            deleteRestoreCode: { recorder.recordDelete() }
        )

        await flow.confirm()

        #expect(recorder.disconnectCalls == 1)
        #expect(recorder.deleteCalls == 1)
        #expect(flow.error == nil)
    }

    @Test("a failed disconnect leaves the restore code in place")
    func confirmFailureKeepsRestoreCode() async {
        let recorder = Recorder()
        let flow = makeFlow(
            disconnect: {
                recorder.recordDisconnect()
                throw StubError.notImplemented
            },
            deleteRestoreCode: { recorder.recordDelete() }
        )

        await flow.confirm()

        #expect(recorder.disconnectCalls == 1)
        #expect(recorder.deleteCalls == 0)
        #expect(flow.error != nil)
    }

    @Test("a superseding prompt cancels the prior warning query")
    func supersedingPromptCancelsPrior() async {
        let recorder = Recorder()
        let flow = makeFlow(warningMessage: {
            if recorder.recordWarning() == 0 {
                // First call: block until cancelled by the second prompt.
                try await Task.sleep(for: .seconds(30))
                return "stale first result"
            }
            return "second result"
        })

        flow.promptDisconnect()
        flow.promptDisconnect()
        await waitUntil { flow.showConfirm }

        #expect(flow.extraWarning == "second result")
        #expect(flow.message.hasSuffix("second result"))
    }
}
