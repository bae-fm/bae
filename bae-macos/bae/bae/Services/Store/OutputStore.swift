import BaeKit
import SwiftUI

/// The stream of the percent each exporting release has reached. The bridge
/// holds each latest percent, and the native view drawing that release's
/// export reads it once per display frame, so a step runs no SwiftUI update
/// and no main-thread hop. The queue's entries are the Storage Manager
/// inspector's, read for one release in core.
@MainActor
@Observable
final class OutputStore {
    /// The latest percent of each exporting release.
    nonisolated let percents: LatestStream<UInt8>

    /// A store whose stream the subscription fills, or, for a preview, a
    /// stream holding fixed percents.
    init(percents: LatestStream<UInt8> = .exportPercents()) {
        self.percents = percents
    }

    /// The latest percent of an exporting release, for the native views that
    /// read it each display frame.
    nonisolated func exportProgress(forRelease releaseId: String)
        -> LatestSource<UInt8>
    {
        percents.source(for: releaseId)
    }
}
