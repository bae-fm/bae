import Foundation
import Observation
import os

private let logger = Logger.bae("BootstrapFollowing")

/// One restore, join or device reset as its screen follows it: where it
/// stands, pulled each time that changes, and the counted step it runs, for
/// the native line that draws it each display frame. Make one for each call,
/// hand the call its `setup`, and stop it once the call ends.
@MainActor
@Observable
public final class BootstrapFollowing {
    /// Where the call stands and whether its Cancel can still stop it; nil
    /// until the call says.
    public private(set) var state: BridgeBootstrapState?
    /// The setup the call is handed.
    @ObservationIgnored
    public let setup: BridgeLibrarySetup
    /// The counted step the call runs, opening or loading, while it runs one.
    @ObservationIgnored
    public let steps: LatestSource<BridgeBootstrapStep>
    @ObservationIgnored
    private let feed: LatestFeed
    @ObservationIgnored
    private var states: ScreenSubscription<SetupStatesSubscription>?

    /// Follow `setup`, made for this one call.
    public init(setup: BridgeLibrarySetup) {
        self.setup = setup
        let stream = LatestStream<BridgeBootstrapStep>.bootstrapSteps()
        feed = stream.feed(from: setup)
        steps = stream.only
        states = ScreenSubscription(
            setup.subscribeStates(),
            onValue: { [weak self] state in self?.state = state },
            onError: { error in
                // The states end only when the setup does; any other failure
                // is the bridge breaking its own guarantee.
                let detail =
                    "a setup's states answered \(String(reflecting: error))"
                logger.error("\(detail)")
                CoreBugReporter()
                    .report(
                        operation: "SetupStatesSubscription.next",
                        detail: detail
                    )
            }
        )
    }

    /// Stop following: the state stays as it last was.
    public func stop() {
        states?.stop()
        states = nil
        feed.cancel()
    }
}
