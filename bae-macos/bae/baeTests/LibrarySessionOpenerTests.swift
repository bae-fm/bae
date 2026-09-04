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

    @Test("a keychain that refused is not a failure the user has to act on")
    func refusedKeychainIsItsOwnOutcome() async {
        let handle = FakeHandle(
            config: makeConfig(),
            keyState: .available,
            keyStateError: BridgeError.Diagnostic(
                category: .keyringLocked,
                detail: "the OS keychain refused the read"
            )
        )
        let opener = TestOpener(
            makeHandle: { _ in handle },
            makeService: { _, _, _ in
                Issue.record("makeService must not run for a refused keychain")
                fatalError("unreachable")
            }
        )

        let waiter = OutcomeWaiter()
        opener.open(libraryId: "lib-1") { waiter.record($0) }
        let outcome = await waiter.value()

        guard case .keychainLocked = outcome else {
            Issue.record(
                "a refused keychain should yield .keychainLocked, got \(outcome)"
            )
            return
        }
    }

    @Test("every other key-state failure is still a failure")
    func otherKeyStateFailuresStillFail() async {
        let handle = FakeHandle(
            config: makeConfig(),
            keyState: .available,
            keyStateError: BridgeError.Diagnostic(
                category: .keyring,
                detail: "the keyring is broken"
            )
        )
        let opener = TestOpener(
            makeHandle: { _ in handle },
            makeService: { _, _, _ in
                Issue.record("makeService must not run for a failed read")
                fatalError("unreachable")
            }
        )

        let waiter = OutcomeWaiter()
        opener.open(libraryId: "lib-1") { waiter.record($0) }
        let outcome = await waiter.value()

        guard case .failed = outcome else {
            Issue.record(
                "a broken keyring should yield .failed, got \(outcome)"
            )
            return
        }
    }

    @Test("a locked target stays retained for unlock")
    func lockedTargetNeedsUnlock() async {
        let handle = FakeHandle(
            config: makeConfig(),
            keyState: .locked
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
        #expect(config.libraryId == "lib-test")
        #expect(!handle.didShutdown)
    }

    @Test("unlock acts on the retained handle and a wrong key can be retried")
    func unlockUsesRetainedHandle() async {
        struct WrongKey: Error {}
        let handle = FakeHandle(
            config: makeConfig(),
            keyState: .locked,
            unlockResults: [.failure(WrongKey()), .success(())],
            outbox: .failure(WrongKey())
        )
        let makeHandleCount = Counter()
        let opener = TestOpener(
            makeHandle: { _ in
                makeHandleCount.increment()
                return handle
            },
            makeService: { _, _, _ in
                Issue.record("outbox failure must precede service creation")
                fatalError("unreachable")
            }
        )

        let open = OutcomeWaiter()
        opener.open(libraryId: "lib-1") { open.record($0) }
        guard case .needsUnlock = await open.value() else {
            Issue.record("locked target did not request unlock")
            return
        }

        await #expect(throws: WrongKey.self) {
            _ = try await opener.unlock(serializedCloudKey: "wrong")
        }
        await #expect(throws: WrongKey.self) {
            _ = try await opener.unlock(serializedCloudKey: "correct")
        }
        #expect(makeHandleCount.value == 1)
        #expect(handle.unlockKeys == ["wrong", "correct"])
        #expect(handle.didShutdown)
    }

    @Test("a superseding open cancels the first, which reports .superseded")
    func supersedingOpenCancelsFirst() async {
        // The first open parks in its injected `initApp` until released, so the
        // second open reliably supersedes it before it can land.
        let gate = DispatchSemaphore(value: 0)
        let parked = FakeHandle(
            config: makeConfig(),
            keyState: .notRequired
        )
        let locked = FakeHandle(
            config: makeConfig(),
            keyState: .locked
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
            config: makeConfig(),
            keyState: .notRequired,
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

    private func makeConfig() -> BridgeConfig {
        BridgeConfig(
            libraryId: "lib-test",
            libraryName: "Test Library",
            libraryPath: "/tmp/test",
            pauseBetweenSides: false,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            identifyAutomatically: true,
            defaultImportMetadataSource: .findOnline,
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            castEnabled: false,
            mcp: BridgeMcpConfig(enabled: false, port: 47777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4533,
                username: "",
                bindAddress: "127.0.0.1"
            ),
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
    let outbox: Result<BridgeOutboxSnapshot, any Error>
    let syncReadyValue: Bool
    private let shutdownFlag = Flag()
    private let state = NSLock()
    private var keyState: BridgeCloudHomeKeyState
    /// Set when the read should refuse rather than answer — a locked OS
    /// keychain throws instead of returning a state, and that difference is the
    /// whole point of the `.keychainLocked` outcome.
    private let keyStateError: (any Error)?
    private var unlockResults: [Result<Void, any Error>]
    private var recordedUnlockKeys: [String] = []

    init(
        config: BridgeConfig,
        keyState: BridgeCloudHomeKeyState,
        keyStateError: (any Error)? = nil,
        unlockResults: [Result<Void, any Error>] = [],
        outbox: Result<BridgeOutboxSnapshot, any Error> = .success(
            OutboxStore.emptySnapshot
        ),
        syncReady: Bool = false
    ) {
        self.config = config
        self.keyState = keyState
        self.keyStateError = keyStateError
        self.unlockResults = unlockResults
        self.outbox = outbox
        syncReadyValue = syncReady
    }

    var didShutdown: Bool { shutdownFlag.isSet }
    var unlockKeys: [String] {
        state.withLock { recordedUnlockKeys }
    }

    func getConfig() -> BridgeConfig { config }
    func cloudHomeKeyState() throws -> BridgeCloudHomeKeyState {
        if let keyStateError { throw keyStateError }
        return state.withLock { keyState }
    }
    func unlockCloudHome(serializedCloudKey: String) async throws {
        let result = state.withLock {
            recordedUnlockKeys.append(serializedCloudKey)
            return unlockResults.removeFirst()
        }
        try result.get()
        state.withLock { keyState = .available }
    }
    func getOutboxSnapshot() async throws -> BridgeOutboxSnapshot {
        try outbox.get()
    }
    func isSyncReady() -> Bool { syncReadyValue }
    func shutdown() async { shutdownFlag.set() }
}

private final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    func increment() {
        lock.withLock { count += 1 }
    }

    var value: Int { lock.withLock { count } }
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
