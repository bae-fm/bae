import Foundation
import Observation
import os.log

private let logger = Logger.bae("DeviceStorageStore")

/// What this device keeps for the open library, read while a screen shows it,
/// and the three things that screen can do about it: clear the cache, set how
/// much audio the cache keeps, and delete the data a reset left behind.
@MainActor
@Observable
public final class DeviceStorageStore {
    /// What the device keeps, or nil before the first value and once no screen
    /// follows it.
    public private(set) var storage: BridgeDeviceStorage?

    @ObservationIgnored
    private let subscribe: @MainActor () -> PulledValues<BridgeDeviceStorage>
    @ObservationIgnored
    private let clear: @Sendable () async throws -> Void
    @ObservationIgnored
    private let limit: @Sendable (BridgeCacheLimitChoice) async throws -> Void
    @ObservationIgnored
    private let deleteBeforeReset: @Sendable () async throws -> Void
    /// Where a failed read or command is shown.
    @ObservationIgnored
    private let showError: @MainActor (any Error) -> Void
    /// How many screens follow the storage now.
    @ObservationIgnored
    private var followers = 0

    public convenience init(
        handle: AppHandle,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.init(
            subscribe: { PulledValues(handle.subscribeDeviceStorage()) },
            clearCache: { try await handle.clearCache() },
            setCacheLimit: { try await handle.setCacheLimit(choice: $0) },
            deleteDataFromBeforeReset: {
                try await handle.deleteDataFromBeforeReset()
            },
            showError: showError
        )
    }

    public init(
        subscribe: @escaping @MainActor () -> PulledValues<BridgeDeviceStorage>,
        clearCache: @escaping @Sendable () async throws -> Void,
        setCacheLimit:
            @escaping @Sendable (BridgeCacheLimitChoice) async throws -> Void,
        deleteDataFromBeforeReset: @escaping @Sendable () async throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.subscribe = subscribe
        clear = clearCache
        limit = setCacheLimit
        deleteBeforeReset = deleteDataFromBeforeReset
        self.showError = showError
    }

    /// Follow the storage until the calling task is cancelled: a screen runs
    /// it while it shows the storage.
    public func follow() async {
        followers += 1
        let pulls = ScreenSubscription(
            subscribe(),
            onValue: { [weak self] storage in self?.storage = storage },
            onError: { [weak self] error in
                logger.error(
                    "device storage read failed: \(String(reflecting: error))"
                )
                self?.showError(error)
            }
        )
        // Held until the task is cancelled, which ends the stream's wait.
        let (cancelled, holding) = AsyncStream.makeStream(of: Never.self)
        for await _ in cancelled {}
        holding.finish()
        pulls.stop()
        followers -= 1
        if followers == 0 { storage = nil }
    }

    /// Remove the cached tracks, covers and pictures; pinned releases stay.
    public func clearCache() async {
        await run("clear the cache", clear)
    }

    /// Keep at most `choice` of cached audio, removing the least recently
    /// played beyond it.
    public func setCacheLimit(_ choice: BridgeCacheLimitChoice) async {
        await run("set the cache limit") { [limit] in try await limit(choice) }
    }

    /// Remove what Reset Device left on this device.
    public func deleteDataFromBeforeReset() async {
        await run("delete the data from before the reset", deleteBeforeReset)
    }

    private func run(
        _ what: String,
        _ command: @Sendable () async throws -> Void
    ) async {
        do {
            try await command()
        }
        catch {
            logger.error(
                "failed to \(what): \(String(reflecting: error))"
            )
            showError(error)
        }
    }

    #if DEBUG
        /// A store whose value is `storage` once a screen follows it, for
        /// previews and shots; its commands do nothing.
        public static func staging(_ storage: BridgeDeviceStorage)
            -> DeviceStorageStore
        {
            DeviceStorageStore(
                subscribe: { PulledValues(FixedDeviceStorage(storage)) },
                clearCache: {},
                setCacheLimit: { _ in },
                deleteDataFromBeforeReset: {},
                showError: { _ in }
            )
        }
    #endif
}

#if DEBUG
    /// A value that answers once and then waits until cancelled.
    private final class FixedDeviceStorage: PulledSubscription,
        @unchecked Sendable
    {
        private let lock = NSLock()
        private var answered = false
        private let storage: BridgeDeviceStorage
        private var waiter: CheckedContinuation<BridgeDeviceStorage, any Error>?

        init(_ storage: BridgeDeviceStorage) { self.storage = storage }

        func next() async throws -> BridgeDeviceStorage {
            let first = lock.withLock {
                defer { answered = true }
                return !answered
            }
            if first { return storage }
            return try await withCheckedThrowingContinuation { continuation in
                lock.withLock { waiter = continuation }
            }
        }

        func cancel() async throws {
            let waiter = lock.withLock {
                defer { self.waiter = nil }
                return self.waiter
            }
            waiter?.resume(throwing: BridgeError.Cancelled)
        }
    }
#endif
