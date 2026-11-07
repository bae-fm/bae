import Foundation
import Observation
import os

private let logger = Logger.bae("StorageSetupFollowing")

/// One storage setup as the app follows it: the bridge's setup, which the
/// setup call is handed, its steps, for the view that draws them each display
/// frame, and whether it is past its point of no return, pulled as that
/// changes. Make one for each call; it stops reading once it is released. Its
/// stream and its pulls run on `host`'s runtime.
@MainActor
@Observable
public final class StorageSetupFollowing {
    /// Whether the setup is past its point of no return, creating the
    /// library in storage or recording changed access: coven finishes it
    /// rather than undoing it, so its screen offers no Cancel.
    public private(set) var finishing = false
    /// The setup the call is handed.
    @ObservationIgnored
    public let setup: BridgeLibrarySetup
    @ObservationIgnored
    public let steps: LatestSource<BridgeStorageSetupProgress>
    /// What feeds `steps`; nil for a staged setup, which nothing feeds.
    @ObservationIgnored
    private let feed: LatestFeed?
    /// The pulls of `finishing`; nil for a staged setup, which nothing pulls.
    @ObservationIgnored
    private var finishingPulls:
        ScreenSubscription<StorageSetupFinishingSubscription>?

    public convenience init(host: BridgeHost) {
        self.init(setup: BridgeLibrarySetup(host: host))
    }

    /// Follow `setup`, made for this one call.
    public init(setup: BridgeLibrarySetup) {
        self.setup = setup
        let stream = LatestStream<BridgeStorageSetupProgress>
            .storageSetupProgress()
        feed = stream.feed(from: setup)
        steps = stream.only
        finishingPulls = ScreenSubscription(
            setup.subscribeStorageFinishing(),
            onValue: { [weak self] finishing in self?.finishing = finishing },
            onError: { error in
                // The pulls end only when the setup does; any other failure
                // is the bridge breaking its own guarantee.
                let detail =
                    "a storage setup's finishing answered \(String(reflecting: error))"
                logger.error("\(detail)")
                CoreBugReporter()
                    .report(
                        operation: "StorageSetupFinishingSubscription.next",
                        detail: detail
                    )
            }
        )
    }

    #if DEBUG
        /// A setup a preview or a shot stages in place of core: it runs
        /// `progress`, past its point of no return when `finishing`, and
        /// nothing reads it.
        public init(
            staging progress: BridgeStorageSetupProgress,
            finishing: Bool
        ) {
            setup = BridgeLibrarySetup(noHandle: BridgeLibrarySetup.NoHandle())
            steps = LatestStream.storageSetupProgress(holding: progress).only
            feed = nil
            self.finishing = finishing
        }
    #endif
}
