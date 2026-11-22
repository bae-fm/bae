import BaeKit
import Foundation
import Observation
import os.log

private let logger = Logger.bae("RatingSystemsStore")

/// How the library shows the ratings of its albums and of its tracks, read
/// while a screen shows it: the album page draws ratings by it, and Settings'
/// Library tab shows it and lets an admin change it.
@MainActor
@Observable
final class RatingSystemsStore {
    /// The systems, or nil before the first value and once no screen follows
    /// them.
    private(set) var systems: BridgeRatingSystems?

    @ObservationIgnored
    private let subscribe: @MainActor () -> PulledValues<BridgeRatingSystems>
    @ObservationIgnored
    private let write:
        @Sendable (BridgeRatedItems, BridgeRatingSystem) async throws -> Void
    /// Where a failed read or change is shown.
    @ObservationIgnored
    private let showError: @MainActor (any Error) -> Void
    /// How many screens follow the systems now.
    @ObservationIgnored
    private var followers = 0

    convenience init(
        handle: AppHandle,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.init(
            subscribe: { PulledValues(handle.subscribeRatingSystems()) },
            setSystem: {
                try await handle.setRatingSystem(items: $0, system: $1)
            },
            showError: showError
        )
    }

    /// `systems` is what the store holds before any screen follows it, which
    /// a shot sets so that its first draw already shows them.
    init(
        systems: BridgeRatingSystems? = nil,
        subscribe: @escaping @MainActor () -> PulledValues<BridgeRatingSystems>,
        setSystem:
            @escaping @Sendable (BridgeRatedItems, BridgeRatingSystem)
            async throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.systems = systems
        self.subscribe = subscribe
        write = setSystem
        self.showError = showError
    }

    /// What a screen draws: the systems, and none for either until the
    /// first value arrives.
    var shown: BridgeRatingSystems {
        systems ?? BridgeRatingSystems(albums: .hidden, tracks: .hidden)
    }

    /// Follow the systems until the calling task is cancelled: a screen runs
    /// it while it shows them.
    func follow() async {
        followers += 1
        let pulls = ScreenSubscription(
            subscribe(),
            onValue: { [weak self] systems in self?.systems = systems },
            onError: { [weak self] error in
                logger.error(
                    "rating systems read failed: \(String(reflecting: error))"
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
        if followers == 0 { systems = nil }
    }

    /// Show the ratings of `items` with `system`, for everyone in the
    /// library. Only an admin can: the library refuses anyone else's change.
    func set(_ items: BridgeRatedItems, to system: BridgeRatingSystem) async {
        do {
            try await write(items, system)
        }
        catch {
            logger.error(
                "rating system change failed: \(String(reflecting: error))"
            )
            showError(error)
        }
    }
}

#if DEBUG
    extension RatingSystemsStore {
        // periphery:ignore - called by baeShots
        /// A store that holds `systems` from the start, for shots and
        /// previews: its read never answers and its changes do nothing. With
        /// none, a screen draws no ratings.
        static func staging(_ systems: BridgeRatingSystems?)
            -> RatingSystemsStore
        {
            RatingSystemsStore(
                systems: systems,
                subscribe: { PulledValues(UnansweredRatingSystems()) },
                setSystem: { _, _ in },
                showError: { _ in }
            )
        }
    }

    /// A read that waits until it is cancelled and never answers.
    private final class UnansweredRatingSystems: PulledSubscription,
        @unchecked Sendable
    {
        private let lock = NSLock()
        private var waiter: CheckedContinuation<BridgeRatingSystems, any Error>?

        func next() async throws -> BridgeRatingSystems {
            try await withCheckedThrowingContinuation { continuation in
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
