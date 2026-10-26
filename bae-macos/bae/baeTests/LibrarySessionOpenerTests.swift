import BaeKit
import Foundation
import Testing

/// Exercises the shared `LibrarySessionOpener` decision flow through its two
/// injected seams — a fake `LibrarySessionHandle` (standing in for the global
/// `initApp` the opener can't otherwise run in a test) and a `makeService`
/// factory that must not run on any of these paths. The happy `.opened` path
/// needs a real `AppService` over a live core, so it isn't unit-tested here; the
/// keychain, supersede, and teardown decisions that precede it are.
@MainActor
@Suite("LibrarySessionOpener")
struct LibrarySessionOpenerTests {
    private typealias TestOpener = LibrarySessionOpener<FakeHandle, AppService>

    @Test("a keychain that refused is not a failure the user has to act on")
    func refusedKeychainIsItsOwnOutcome() async {
        let opener = TestOpener(
            makeHandle: { _ in
                throw BridgeError.Diagnostic(
                    category: .keyringLocked,
                    detail: "the OS keychain refused the read"
                )
            },
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

    @Test("every other open failure is still a failure")
    func otherOpenFailuresStillFail() async {
        let opener = TestOpener(
            makeHandle: { _ in
                throw BridgeError.Diagnostic(
                    category: .keyring,
                    detail: "the keyring is broken"
                )
            },
            makeService: { _, _, _ in
                Issue.record("makeService must not run for a failed open")
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

    @Test("a superseding open cancels the first, which reports .superseded")
    func supersedingOpenCancelsFirst() async {
        struct OpenError: Error {}
        // The first open parks in its injected `initApp` until released, so the
        // second open reliably supersedes it before it can land.
        let gate = DispatchSemaphore(value: 0)
        let parked = FakeHandle(config: makeConfig())
        let opener = TestOpener(
            makeHandle: { libraryId in
                guard libraryId == "parked" else { throw OpenError() }
                gate.wait()
                return parked
            },
            makeService: { _, _, _ in
                Issue.record("makeService must not run in the supersede test")
                fatalError("unreachable")
            }
        )

        let first = OutcomeWaiter()
        let second = OutcomeWaiter()
        opener.open(libraryId: "parked") { first.record($0) }
        opener.open(libraryId: "failing") { second.record($0) }

        // The second open runs to completion while the first is parked.
        let secondOutcome = await second.value()
        guard case .failed = secondOutcome else {
            Issue.record(
                "second open should yield .failed, got \(secondOutcome)"
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
    }

    @Test("a failed outbox seed closes the library and reports .failed")
    func outboxSeedFailureClosesTheLibrary() async {
        struct SeedError: Error {}
        let handle = FakeHandle(
            config: makeConfig(),
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
        #expect(handle.didClose)
    }

    @Test("a failed name read closes the library and reports .failed")
    func nameSeedFailureClosesTheLibrary() async {
        struct NameError: Error {}
        let handle = FakeHandle(
            config: makeConfig(),
            name: .failure(NameError())
        )
        let opener = TestOpener(
            makeHandle: { _ in handle },
            makeService: { _, _, _ in
                Issue.record("makeService must not run when the name fails")
                fatalError("unreachable")
            }
        )

        let waiter = OutcomeWaiter()
        opener.open(libraryId: "lib-4") { waiter.record($0) }
        let outcome = await waiter.value()

        guard case .failed(let error) = outcome else {
            Issue.record(
                "a failed name read should yield .failed, got \(outcome)"
            )
            return
        }
        #expect(error is NameError)
        #expect(handle.didClose)
    }

    @Test("a bootstrapped store opens its own handle, not one by id")
    func retainedHandleSkipsTheFactory() async {
        struct SeedError: Error {}
        let retained = FakeHandle(
            config: makeConfig(),
            outbox: .failure(SeedError())
        )
        let opener = TestOpener(
            makeHandle: { _ in
                Issue.record("a bootstrapped store must not reopen by id")
                fatalError("unreachable")
            },
            makeService: { _, _, _ in
                Issue.record("makeService must not run when the seed fails")
                fatalError("unreachable")
            }
        )

        let waiter = OutcomeWaiter()
        opener.open(
            makeHandle: { retained },
            onOutcome: { waiter.record($0) }
        )
        guard case .failed = await waiter.value() else {
            Issue.record("the retained handle's failed seed should fail")
            return
        }
        #expect(retained.didClose)
    }
}

// MARK: - Fixtures

extension LibrarySessionOpenerTests {
    private func makeConfig() -> BridgeConfig {
        BridgeConfig(
            libraryId: "lib-test",
            libraryPath: "/tmp/test",
            pauseBetweenSides: false,
            sidePauseCountdown: .off,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            identifyAutomatically: true,
            importWhenIdentified: false,
            importStorage: BridgeImportStorage(
                cloud: true,
                pinned: true
            ),
            lookupCatalogs: [
                BridgeLookupCatalogSetting(
                    catalog: .musicBrainz,
                    availability: .on,
                    canChange: true
                ),
                BridgeLookupCatalogSetting(
                    catalog: .discogs,
                    availability: .on,
                    canChange: true
                ),
            ],
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            castEnabled: false,
            castTranscodeFormat: .mp3,
            mcp: BridgeMcpConfig(enabled: false, port: 47777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4533,
                username: "",
                bindAddress: "127.0.0.1"
            ),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false
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

/// A hand-built `LibrarySessionHandle` for the opener tests: it answers the
/// reads the opener makes off a real `AppHandle` and records whether its library
/// was closed, so a test can assert the seed-failure teardown ran.
private final class FakeHandle: LibrarySessionHandle, @unchecked Sendable {
    let config: BridgeConfig
    let name: Result<String, any Error>
    let outbox: Result<BridgeOutboxSnapshot, any Error>
    private let closeFlag = Flag()

    init(
        config: BridgeConfig,
        name: Result<String, any Error> = .success("Test Library"),
        outbox: Result<BridgeOutboxSnapshot, any Error> = .success(
            OutboxStore.emptySnapshot
        )
    ) {
        self.config = config
        self.name = name
        self.outbox = outbox
    }

    var didClose: Bool { closeFlag.isSet }

    func getConfig() -> BridgeConfig { config }
    func libraryName() async throws -> String { try name.get() }
    func storageLocation() async throws -> BridgeStorageLocation? { nil }
    func getOutboxSnapshot() async throws -> BridgeOutboxSnapshot {
        try outbox.get()
    }
    func closeLibrary() throws { closeFlag.set() }
}

/// A one-way flag settable from any thread — the opener calls `closeLibrary()`
/// on the fake off the main actor, so the write and the test's later read cross
/// threads.
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
