import BaeKit
import Foundation
import Observation
import os.log

private let logger = Logger.bae("LibraryNameStore")

/// The library's name, read while a screen shows it: Settings' Library tab
/// follows it while the tab shows, and Rename Library starts from the name
/// as it stands when the sheet opens.
@MainActor
@Observable
final class LibraryNameStore {
    /// The name the screen following it shows, or nil before its first
    /// value and once no screen follows it.
    private(set) var name: String?

    @ObservationIgnored
    private let subscribe: @MainActor () -> PulledValues<String>
    @ObservationIgnored
    private let readCurrent: @Sendable () async throws -> String
    /// Where a failed read of the name is shown.
    @ObservationIgnored
    private let showError: @MainActor (any Error) -> Void
    /// How many screens follow the name now.
    @ObservationIgnored
    private var followers = 0

    convenience init(
        handle: AppHandle,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.init(
            subscribe: { PulledValues(handle.subscribeLibraryName()) },
            current: { try await handle.libraryName() },
            showError: showError
        )
    }

    init(
        subscribe: @escaping @MainActor () -> PulledValues<String>,
        current: @escaping @Sendable () async throws -> String,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.subscribe = subscribe
        readCurrent = current
        self.showError = showError
    }

    /// Follow the name until the calling task is cancelled: a screen runs it
    /// while it shows the name.
    func follow() async {
        followers += 1
        let pulls = ScreenSubscription(
            subscribe(),
            onValue: { [weak self] name in self?.name = name },
            onError: { [weak self] error in
                logger.error(
                    "library name read failed: \(String(reflecting: error))"
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
        if followers == 0 { name = nil }
    }

    /// The name as it stands now, for an editor that starts from it.
    func current() async throws -> String {
        try await readCurrent()
    }

    #if DEBUG
        /// A store whose name is `name` once a screen follows it, for
        /// shots.
        static func stub(name: String) -> LibraryNameStore {
            LibraryNameStore(
                subscribe: { PulledValues(FixedLibraryName(name: name)) },
                current: { name },
                showError: { _ in }
            )
        }
    #endif
}

#if DEBUG
    /// A name that answers once and then waits until cancelled.
    private final class FixedLibraryName: PulledSubscription,
        @unchecked Sendable
    {
        private let lock = NSLock()
        private var answered = false
        private let name: String
        private var waiter: CheckedContinuation<String, any Error>?

        init(name: String) { self.name = name }

        func next() async throws -> String {
            let first = lock.withLock {
                defer { answered = true }
                return !answered
            }
            if first { return name }
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
