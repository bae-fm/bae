import Foundation
import Observation

/// Everything sync is doing on this device, as core reports it: the title
/// bar's button and the panel it opens read it here.
@MainActor
@Observable
public final class SyncActivityStore {
    public private(set) var activity: BridgeSyncActivity?
    /// Why the activity couldn't be read, until it is read again.
    public private(set) var error: DisplayError?
    /// The share of each transfer line's batch bytes done, by line.
    public nonisolated let bars: LatestStream<Double>

    /// `bars` holds the share of each line's batch a preview shows.
    public init(
        activity: BridgeSyncActivity? = nil,
        bars: LatestStream<Double> = .activityBars()
    ) {
        self.activity = activity
        self.bars = bars
    }

    /// The latest share of a transfer line's batch bytes done, for the native
    /// bar that reads it each display frame. Core keys each line's bar by the
    /// line's number as decimal text.
    public nonisolated func bar(_ line: BridgeActivityLine)
        -> LatestSource<Double>
    {
        bars.source(for: String(line.rawValue))
    }

    public func apply(_ activity: BridgeSyncActivity) {
        self.activity = activity
        error = nil
    }

    public func fail(_ error: any Error) {
        self.error = DisplayError(error)
    }
}
