import BaeKit
import Foundation
import Testing

@testable import bae

/// Covers `RestoreCodeReveal`, the Settings restore code's closed → loading
/// → masked → shown cycle, against stubbed generation — no live core. The load-bearing
/// invariants are that a code generated after the screen closed, or after a
/// newer request replaced the run, never shows.
@MainActor
@Suite("RestoreCodeReveal", .timeLimit(.minutes(1)))
struct RestoreCodeRevealTests {
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
                lock.recording {
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
        BridgeRestoreCodeShare(code: code, qrText: code)
    }

    @Test("starts closed, holds the code masked once it is generated")
    func holdsTheGeneratedCodeMasked() async throws {
        let reveal = RestoreCodeReveal()
        #expect(reveal.state == .closed)

        reveal.open { Self.share("CVR1 SHOWN") }
        #expect(reveal.state == .loading)

        try await Wait.until { reveal.state == .masked(code: "CVR1 SHOWN") }
    }

    @Test("show reveals the held code and hide masks it again")
    func showAndHide() async throws {
        let reveal = RestoreCodeReveal()
        reveal.open { Self.share("CVR1 HELD") }
        try await Wait.until { reveal.state == .masked(code: "CVR1 HELD") }

        reveal.show()
        #expect(reveal.state == .shown(code: "CVR1 HELD"))
        reveal.hide()
        #expect(reveal.state == .masked(code: "CVR1 HELD"))
    }

    @Test("a failure core gave a line shows that line")
    func failureShowsItsLine() async throws {
        let reveal = RestoreCodeReveal()
        let failure = BridgeError.Diagnostic(
            category: .config,
            detail: "storage unreachable"
        )
        let shown = try #require(DisplayError(failure))
        reveal.open { throw failure }

        try await Wait.until { reveal.state == .failed(shown) }
    }

    @Test("a cancellation from core leaves the section closed")
    func coreCancellationCloses() async throws {
        let reveal = RestoreCodeReveal()
        reveal.open { throw BridgeError.Cancelled }

        try await Wait.until { reveal.state == .closed }
    }

    @Test("a code generated after the screen closed stays dropped")
    func closedWhileGeneratingStaysClosed() async throws {
        let reveal = RestoreCodeReveal()
        let gate = Gate()
        reveal.open {
            await gate.pass()
            return Self.share("CVR1 LATE")
        }
        try await Wait.until { gate.entered }

        reveal.close()
        #expect(reveal.state == .closed)

        gate.open()
        await reveal.generationsEnded()
        #expect(reveal.state == .closed)
    }

    @Test("a newer request replaces a run still generating")
    func newerRequestWins() async throws {
        let reveal = RestoreCodeReveal()
        let gate = Gate()
        reveal.open {
            await gate.pass()
            return Self.share("CVR1 OLD")
        }
        try await Wait.until { gate.entered }

        reveal.open { Self.share("CVR1 NEW") }
        try await Wait.until { reveal.state == .masked(code: "CVR1 NEW") }

        gate.open()
        await reveal.generationsEnded()
        #expect(reveal.state == .masked(code: "CVR1 NEW"))
    }
}

/// How a restore code shows while it is masked.
@Suite("RestoreCodeMask")
struct RestoreCodeMaskTests {
    @Test("keeps the first two groups and the last, eliding the rest")
    func masksTheMiddle() {
        #expect(
            RestoreCodeMask.masked("CVR1 UJZD EGXD NCF3 2EPF 3DHO")
                == RestoreCodeMask.Parts(
                    leading: "CVR1 UJZD •••• •••• ••••",
                    trailing: "3DHO"
                )
        )
    }

    @Test("a code too short to elide hides every group")
    func shortCodeIsAllBullets() {
        #expect(
            RestoreCodeMask.masked("CVR1 UJZD EGXD")
                == RestoreCodeMask.Parts(
                    leading: "•••• •••• ••••",
                    trailing: ""
                )
        )
    }
}
