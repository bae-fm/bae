import BaeKit
import Foundation
import Observation
import os.log

private let logger = Logger.bae("RatingSystemsStore")

/// How the library shows the ratings of its albums and of its tracks, read
/// while Settings' Library tab shows it, which lets an admin change it. The
/// album page needs no read of it: core delivers each album and track with
/// its rating already shown by the library's system.
@MainActor
@Observable
final class RatingSystemsStore {
    /// The systems, or nil before the first value and once the tab no longer
    /// follows them.
    private(set) var systems: BridgeRatingSystems?
    /// The system the person chose for albums or for tracks, until its write
    /// returns.
    private(set) var pending: [BridgeRatedItems: BridgeRatingSystem] = [:]

    @ObservationIgnored
    private let subscribe: @MainActor () -> PulledValues<BridgeRatingSystems>
    @ObservationIgnored
    private let write:
        @Sendable (BridgeRatedItems, BridgeRatingSystem) async throws -> Void
    /// Where a failed read or change is shown.
    @ObservationIgnored
    private let showError: @MainActor (any Error) -> Void

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

    /// `systems` is what the store holds before the tab follows it, which a
    /// shot sets so that its first draw already shows them.
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

    /// Follow the systems until the calling task is cancelled: the one view
    /// that shows them runs it.
    func follow() async {
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
        await Task.untilCancelled()
        pulls.stop()
        systems = nil
    }

    /// Show the ratings of `items` with `system`, for everyone in the
    /// library. Only an admin can: the library refuses anyone else's change.
    /// A choice for `items` waits out an earlier one for the same items.
    func choose(_ items: BridgeRatedItems, _ system: BridgeRatingSystem) {
        guard pending[items] == nil else { return }
        pending[items] = system
        // The write belongs to the person's choice, not to the tab: the
        // library saves it even when they leave Settings right away.
        Task {
            do {
                try await write(items, system)
            }
            catch {
                logger.error(
                    "rating system change failed: \(String(reflecting: error))"
                )
                showError(error)
            }
            pending[items] = nil
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
                subscribe: {
                    PulledValues(read: { try await Task.neverAnswers() })
                },
                setSystem: { _, _ in },
                showError: { _ in }
            )
        }
    }
#endif
