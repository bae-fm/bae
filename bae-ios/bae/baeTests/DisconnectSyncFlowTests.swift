import BaeKit
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
    /// Sendable` because the `disconnect` closure crosses the async bridge
    /// boundary.
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
        cloudOnlyReleaseCount: @escaping @Sendable () async throws -> UInt64 = {
            0
        },
        atRiskMessage: @escaping (UInt64) -> String = { "\($0) at risk." },
        disconnect: @escaping @Sendable () async throws -> Void = {},
        deleteRestoreCode: @escaping () throws -> Void = {},
        baseMessage: @escaping () -> String = { "Disconnect." },
        warningCheckFailedMessage: @escaping (String) -> String = {
            "check failed: \($0)"
        },
        disconnectFailedMessage: @escaping (String) -> String = {
            "disconnect failed: \($0)"
        },
        restoreCodeDeleteFailedMessage: @escaping (String) -> String = {
            "restore code delete failed: \($0)"
        }
    ) -> DisconnectSyncFlow {
        DisconnectSyncFlow(
            cloudOnlyReleaseCount: cloudOnlyReleaseCount,
            atRiskMessage: atRiskMessage,
            disconnect: disconnect,
            deleteRestoreCode: deleteRestoreCode,
            baseMessage: baseMessage,
            warningCheckFailedMessage: warningCheckFailedMessage,
            disconnectFailedMessage: disconnectFailedMessage,
            restoreCodeDeleteFailedMessage: restoreCodeDeleteFailedMessage
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
        let warning = "2 at risk."
        let flow = makeFlow(cloudOnlyReleaseCount: { 2 })
        let base = flow.message

        flow.promptDisconnect()
        await waitUntil { flow.showConfirm }

        #expect(flow.extraWarning == warning)
        #expect(flow.message == "\(base) \(warning)")
    }

    @Test("prompt with no at-risk releases shows the base copy only")
    func promptWithoutWarning() async {
        let flow = makeFlow(cloudOnlyReleaseCount: { 0 })
        let base = flow.message

        flow.promptDisconnect()
        await waitUntil { flow.showConfirm }

        #expect(flow.extraWarning == nil)
        #expect(flow.message == base)
    }

    @Test("a failed at-risk check still opens the confirmation, with an error")
    func promptWarningFailureStillOpens() async {
        let flow = makeFlow(cloudOnlyReleaseCount: {
            throw StubError.notImplemented
        })

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

    @Test("a keychain that refuses the delete says so, not \"disconnect failed\"")
    func confirmSurfacesRestoreCodeDeleteFailure() async {
        let recorder = Recorder()
        let flow = makeFlow(
            disconnect: { recorder.recordDisconnect() },
            deleteRestoreCode: {
                recorder.recordDelete()
                throw StubError.notImplemented
            }
        )

        await flow.confirm()

        #expect(recorder.disconnectCalls == 1)
        #expect(recorder.deleteCalls == 1)
        #expect(flow.error?.hasPrefix("restore code delete failed: ") == true)
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
        let flow = makeFlow(cloudOnlyReleaseCount: {
            if recorder.recordWarning() == 0 {
                // First call: block until cancelled by the second prompt.
                try await Task.sleep(for: .seconds(30))
                return 99
            }
            return 2
        })

        flow.promptDisconnect()
        flow.promptDisconnect()
        await waitUntil { flow.showConfirm }

        #expect(flow.extraWarning == "2 at risk.")
        #expect(flow.message.hasSuffix("2 at risk."))
    }
}
