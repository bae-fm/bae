import BaeKit
import SwiftUI

/// The percent each exporting release has reached. The transfer subscription
/// writes each latest percent on the thread it arrives on, and the native
/// view drawing that release's export reads it once per display frame, so a
/// step runs no SwiftUI update and no main-thread hop. The queue's entries
/// are the Storage Manager inspector's, read for one release in core.
@MainActor
@Observable
final class OutputStore {
    /// The latest percent of each exporting release.
    private nonisolated let percents = LatestByKey<UInt8>()

    /// Apply one change the transfer subscription delivered, on any thread.
    nonisolated func applyTransferChange(_ change: BridgeOutputTransferChange) {
        switch change {
        case .updated(let releaseId, let percent):
            percents.set(percent, for: releaseId)
        case .removed(let releaseId):
            percents.remove(releaseId)
        case .reset(let keyed):
            percents.reset(
                Dictionary(
                    uniqueKeysWithValues: keyed.map {
                        ($0.releaseId, $0.percent)
                    }
                )
            )
        }
    }

    /// The latest percent of an exporting release, for the native views that
    /// read it each display frame.
    nonisolated func exportProgress(forRelease releaseId: String)
        -> LatestSource<UInt8>
    {
        LatestSource(store: percents, key: releaseId)
    }
}
