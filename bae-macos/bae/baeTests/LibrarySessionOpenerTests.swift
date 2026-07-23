import BaeKit
import Foundation
import Testing

/// Exercises the shared `LibrarySessionOpener` decision flow through its two
/// injected seams — a fake `LibrarySessionHandle` (standing in for the global
/// `initApp` the opener can't otherwise run in a test) and a `makeService`
/// factory that must not run on any of these paths. The happy `.opened` path
/// needs a real `AppService` over a live core, so it isn't unit-tested here; the
/// gate, supersede, and teardown decisions that precede it are.
@MainActor
@Suite("LibrarySessionOpener")
struct LibrarySessionOpenerTests {
    private typealias TestOpener = LibrarySessionOpener<FakeHandle, AppService>

    @Test(
        "a locked target yields .needsUnlock without building or tearing down"
    )
    func lockedTargetNeedsUnlock() async {
        let handle = FakeHandle(
            config: makeConfig(encryptionKeyStored: true, fingerprint: "fp-1"),
            hasEncryptionKey: false
        )
        let opener = TestOpener(
            makeHandle: { _ in handle },
            makeService: { _, _, _ in
                Issue.record("makeService must not run for a locked target")
                fatalError("unreachable")
            }
        )

        let waiter = OutcomeWaiter()
        opener.open(libraryId: "lib-1") { waiter.record($0) }
        let outcome = await waiter.value()

        guard case .needsUnlock(let config) = outcome else {
            Issue.record(
                "locked target should yield .needsUnlock, got \(outcome)"
            )
            return
        }
        #expect(config.encryptionKeyStored)
        // The locked path drops the handle by releasing it, not by shutting it
        // down; an unlock retry then reopens the same library.
        #expect(!handle.didShutdown)
    }

    @Test("a superseding open cancels the first, which reports .superseded")
    func supersedingOpenCancelsFirst() async {
        // The first open parks in its injected `initApp` until released, so the
        // second open reliably supersedes it before it can land.
        let gate = DispatchSemaphore(value: 0)
        let parked = FakeHandle(
            config: makeConfig(encryptionKeyStored: false),
            hasEncryptionKey: true
        )
        let locked = FakeHandle(
            config: makeConfig(encryptionKeyStored: true, fingerprint: "fp-2"),
            hasEncryptionKey: false
        )
        let opener = TestOpener(
            makeHandle: { libraryId in
                if libraryId == "parked" { gate.wait() }
                return libraryId == "parked" ? parked : locked
            },
            makeService: { _, _, _ in
                Issue.record("makeService must not run in the supersede test")
                fatalError("unreachable")
            }
        )

        let first = OutcomeWaiter()
        let second = OutcomeWaiter()
        opener.open(libraryId: "parked") { first.record($0) }
        opener.open(libraryId: "locked") { second.record($0) }

        // The second open runs to completion while the first is parked.
        let secondOutcome = await second.value()
        guard case .needsUnlock = secondOutcome else {
            Issue.record(
                "second open should yield .needsUnlock, got \(secondOutcome)"
            )
            return
        }

        // Release the first; it resumes past its cancellation check and reports
        // that it was superseded, never landing its handle.
        gate.signal()
        let firstOutcome = await first.value()
        guard case .superseded = firstOutcome else {
            Issue.record("first open should be superseded, got \(firstOutcome)")
            return
        }
        #expect(!parked.didShutdown)
    }

    @Test("a failed outbox seed shuts the handle down and reports .failed")
    func outboxSeedFailureShutsDown() async {
        struct SeedError: Error {}
        let handle = FakeHandle(
            config: makeConfig(encryptionKeyStored: false),
            hasEncryptionKey: true,
            outbox: .failure(SeedError())
        )
        let opener = TestOpener(
            makeHandle: { _ in handle },
            makeService: { _, _, _ in
                Issue.record(
                    "makeService must not run when the outbox seed fails"
                )
                fatalError("unreachable")
            }
        )

        let waiter = OutcomeWaiter()
        opener.open(libraryId: "lib-3") { waiter.record($0) }
        let outcome = await waiter.value()

        guard case .failed(let error) = outcome else {
            Issue.record(
                "a failed outbox seed should yield .failed, got \(outcome)"
            )
            return
        }
        #expect(error is SeedError)
        #expect(handle.didShutdown)
    }

    // MARK: - Fixtures

    private func makeConfig(
        encryptionKeyStored: Bool,
        fingerprint: String? = nil
    ) -> BridgeConfig {
        BridgeConfig(
            libraryId: "lib-test",
            libraryName: "Test Library",
            libraryPath: "/tmp/test",
            encryptionKeyStored: encryptionKeyStored,
            encryptionKeyFingerprint: fingerprint,
            pauseBetweenSides: false,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            showRemainingTime: false,
                    libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            mcp: BridgeMcpConfig(enabled: false, port: 47777),
            subsonic: BridgeSubsonicConfig(enabled: false, port: 4533, username: ""),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false,
            sync: nil
        )
    }

    /// Collects the single outcome one `open` delivers and lets a test await it,
    /// whichever order the delivery and the await happen in. The outcome is not
    /// `Sendable` (it carries `any Error`), so it stays on the main actor: the
    /// continuation only signals arrival, and the value is read back here.
    @MainActor
    private final class OutcomeWaiter {
        private var stored: TestOpener.Outcome?
        private var arrival: CheckedContinuation<Void, Never>?

        func record(_ outcome: TestOpener.Outcome) {
            stored = outcome
            arrival?.resume()
            arrival = nil
        }

        func value() async -> TestOpener.Outcome {
            if stored == nil {
                await withCheckedContinuation { arrival = $0 }
            }
            guard let stored else {
                fatalError("outcome recorded before the arrival signal resumed")
            }
            return stored
        }
    }
}

/// A hand-built `LibrarySessionHandle` for the opener tests: it answers the five
/// reads the opener makes off a real `AppHandle` and records whether it was shut
/// down, so a test can assert the outbox-failure teardown ran (or, on the other
/// paths, didn't).
private final class FakeHandle: LibrarySessionHandle, @unchecked Sendable {
    let config: BridgeConfig
    let encryptionKeyPresent: Bool
    let outbox: Result<BridgeOutboxSnapshot, any Error>
    let syncReadyValue: Bool
    private let shutdownFlag = Flag()

    init(
        config: BridgeConfig,
        hasEncryptionKey: Bool,
        outbox: Result<BridgeOutboxSnapshot, any Error> = .success(
            OutboxStore.emptySnapshot
        ),
        syncReady: Bool = false
    ) {
        self.config = config
        encryptionKeyPresent = hasEncryptionKey
        self.outbox = outbox
        syncReadyValue = syncReady
    }

    var didShutdown: Bool { shutdownFlag.isSet }

    func getConfig() -> BridgeConfig { config }
    func hasEncryptionKey() -> Bool { encryptionKeyPresent }
    func getOutboxSnapshot() async throws -> BridgeOutboxSnapshot {
        try outbox.get()
    }
    func isSyncReady() -> Bool { syncReadyValue }
    func shutdown() async { shutdownFlag.set() }
}

/// A one-way flag settable from any thread — the opener awaits `shutdown()` off
/// the fake, so the write and the test's later read cross threads.
private final class Flag: @unchecked Sendable {
    private let lock = NSLock()
    private var value = false

    func set() {
        lock.lock()
        value = true
        lock.unlock()
    }

    var isSet: Bool {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}
