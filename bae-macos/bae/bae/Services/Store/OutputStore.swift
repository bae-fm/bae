import BaeKit
import SwiftUI

/// The percent each exporting release has reached. The transfer subscription
/// lands each percent in a store of keyed values nothing observes, and the
/// native view drawing that release's export sets itself from it, so a step
/// runs no SwiftUI update. The queue's entries are the Storage Manager
/// inspector's, read for one release in core.
@MainActor
@Observable
final class OutputStore {
    /// Each exporting release's percent, by release.
    private let percents = KeyedValues<UInt8>()
    /// The latest percent of each exporting release, which the release's
    /// native views read each display frame.
    private nonisolated let latestPercents = LatestByKey<UInt8>()

    /// Apply one change the transfer subscription delivered.
    func applyTransferChange(_ change: BridgeOutputTransferChange) {
        switch change {
        case .updated(let releaseId, let percent):
            percents.set(percent, for: releaseId)
            latestPercents.set(percent, for: releaseId)
        case .removed(let releaseId):
            percents.remove(releaseId)
            latestPercents.remove(releaseId)
        case .reset(let keyed):
            let all = Dictionary(
                uniqueKeysWithValues: keyed.map { ($0.releaseId, $0.percent) }
            )
            percents.reset(all)
            latestPercents.reset(all)
        }
    }

    /// The latest percent of an exporting release, for the native views that
    /// read it each display frame.
    nonisolated func exportProgress(forRelease releaseId: String)
        -> LatestSource<UInt8>
    {
        LatestSource(store: latestPercents, key: releaseId)
    }

    /// The stream of a release's export percent, for the native view that
    /// draws it.
    func percent(forRelease releaseId: String) -> KeyedValueSource<UInt8> {
        KeyedValueSource(values: percents, key: releaseId)
    }
}
