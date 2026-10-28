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

    /// Apply one change the transfer subscription delivered.
    func applyTransferChange(_ change: BridgeOutputTransferChange) {
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

    /// The stream of a release's export percent, for the native view that
    /// draws it.
    func percent(forRelease releaseId: String) -> KeyedValueSource<UInt8> {
        KeyedValueSource(values: percents, key: releaseId)
    }
}
