import BaeKit
import Foundation
import Testing

@testable import bae

/// Covers `RecoveryCodeReveal`, the Settings recovery code's hidden → loading
/// → shown cycle, against stubbed generation — no live core. The load-bearing
/// invariants are that a code generated after the person hid it, or after a
/// newer request replaced the run, never shows.
@MainActor
@Suite("RecoveryCodeReveal", .timeLimit(.minutes(1)))
struct RecoveryCodeRevealTests {
    /// Holds a generation until the test opens it, without observing
    /// cancellation, so a run can finish after it was cancelled.
    private final class Gate: @unchecked Sendable {
        private let lock = NSLock()
        private var waiting: CheckedContinuation<Void, Never>?
        private var isOpen = false
        private var _entered = false

        /// Whether a generation is waiting at (or went through) the gate.
        var entered: Bool { lock.withLock { _entered } }

        func pass() async {
            await withCheckedContinuation { continuation in
                lock.withLock {
                    _entered = true
                    if isOpen {
                        continuation.resume()
                    }
                    else {
                        waiting = continuation
                    }
                }
            }
        }

        func open() {
            lock.withLock {
                isOpen = true
                waiting?.resume()
                waiting = nil
            }
        }
    }

    nonisolated private static func share(_ code: String)
        -> BridgeRestoreCodeShare
    {
        BridgeRestoreCodeShare(code: code, qrText: code, qrCarriesS3Key: false)
    }

    @Test("starts hidden and shows the code once it is generated")
    func showsTheGeneratedCode() async throws {
        let reveal = RecoveryCodeReveal()
        #expect(reveal.state == .hidden)

        reveal.show { Self.share("CVR1-SHOWN") }
        #expect(reveal.state == .loading)

        try await Wait.until { reveal.state == .shown(code: "CVR1-SHOWN") }
    }

    @Test("a failure core gave a line shows that line")
    func failureShowsItsLine() async throws {
        let reveal = RecoveryCodeReveal()
        let failure = BridgeError.Diagnostic(
            category: .config,
            detail: "storage unreachable"
        )
        let shown = try #require(DisplayError(failure))
        reveal.show { throw failure }

        try await Wait.until { reveal.state == .failed(shown) }
    }

    @Test("a cancellation from core leaves the code hidden")
    func coreCancellationHides() async throws {
        let reveal = RecoveryCodeReveal()
        reveal.show { throw BridgeError.Cancelled }

        try await Wait.until { reveal.state == .hidden }
    }

    @Test("a code generated after the person hid it stays hidden")
    func hiddenWhileGeneratingStaysHidden() async throws {
        let reveal = RecoveryCodeReveal()
        let gate = Gate()
        reveal.show {
            await gate.pass()
            return Self.share("CVR1-LATE")
        }
        try await Wait.until { gate.entered }

        reveal.hide()
        #expect(reveal.state == .hidden)

        gate.open()
        try await Wait.untilSteady { reveal.state }
        #expect(reveal.state == .hidden)
    }

    @Test("a newer request replaces a run still generating")
    func newerRequestWins() async throws {
        let reveal = RecoveryCodeReveal()
        let gate = Gate()
        reveal.show {
            await gate.pass()
            return Self.share("CVR1-OLD")
        }
        try await Wait.until { gate.entered }

        reveal.show { Self.share("CVR1-NEW") }
        try await Wait.until { reveal.state == .shown(code: "CVR1-NEW") }

        gate.open()
        try await Wait.untilSteady { reveal.state }
        #expect(reveal.state == .shown(code: "CVR1-NEW"))
    }
}
