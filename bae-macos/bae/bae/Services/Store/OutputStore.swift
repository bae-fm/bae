import BaeKit
import SwiftUI

/// One release's export percent, observed on its own: a step changes only
/// this value, so only a view that draws it redraws.
@Observable
final class OutputTransferValue {
    private(set) var percent: UInt8

    init(_ percent: UInt8) {
        self.percent = percent
    }

    fileprivate func set(_ percent: UInt8) {
        self.percent = percent
    }
}

/// The percent each exporting release has reached. The transfer subscription
/// lands each release's percent into that release's own value, so a step
/// redraws only what draws that release's export. The queue's entries are
/// the Storage Manager inspector's, read for one release in core.
@MainActor
@Observable
final class OutputStore {
    /// Each exporting release's percent, one value per release; the map
    /// changes only as releases start and stop exporting.
    private var transfers: [String: OutputTransferValue] = [:]
    /// Each exporting release's percent, by release, for the native views
    /// that draw it; nothing SwiftUI observes.
    private let percents = KeyedValues<UInt8>()

    /// Apply one change the transfer subscription delivered.
    func applyTransferChange(_ change: BridgeOutputTransferChange) {
        switch change {
        case .updated(let releaseId, let percent):
            if let value = transfers[releaseId] {
                value.set(percent)
            }
            else {
                transfers[releaseId] = OutputTransferValue(percent)
            }
            percents.set(percent, for: releaseId)
        case .removed(let releaseId):
            transfers.removeValue(forKey: releaseId)
            percents.remove(releaseId)
        case .reset(let keyed):
            transfers = Dictionary(
                uniqueKeysWithValues: keyed.map {
                    ($0.releaseId, OutputTransferValue($0.percent))
                }
            )
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

    /// A release's export percent as its own observed value, for a view to
    /// hand to the one leaf that draws it: reading this follows only the
    /// release starting and stopping its export. `nil` while none has
    /// arrived for it.
    func transfer(forRelease releaseId: String) -> OutputTransferValue? {
        transfers[releaseId]
    }
}
