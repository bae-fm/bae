import SwiftUI

/// One release's download progress, observed on its own: bytes arriving
/// change only this value, so only a view that draws it redraws.
@Observable
public final class DownloadTransferValue {
    public internal(set) var progress: BridgeDownloadTransferProgress

    init(_ progress: BridgeDownloadTransferProgress) {
        self.progress = progress
    }
}

/// Mirror of core's download (pin) queue, rendered by the Storage Manager's
/// Downloads pane. The retained download subscription lands the queue —
/// entries, phases, pause — each time it changes, and it is swapped
/// wholesale; the transfer subscription lands each downloading release's
/// bytes into that release's own value, so bytes arriving redraw only what
/// draws that release's progress. Views read both at the leaf.
@Observable
public class DownloadStore {
    public var snapshot: BridgeDownloadSnapshot
    /// Each downloading release's progress, one value per release; the map
    /// changes only as releases start and stop downloading.
    private var transfers: [String: DownloadTransferValue] = [:]

    public init(snapshot: BridgeDownloadSnapshot) {
        self.snapshot = snapshot
    }

    public func applySnapshot(_ snapshot: BridgeDownloadSnapshot) {
        self.snapshot = snapshot
    }

    /// Apply one change the transfer subscription delivered.
    public func applyTransferChange(_ change: BridgeDownloadTransferChange) {
        switch change {
        case .updated(let releaseId, let progress):
            if let value = transfers[releaseId] {
                value.progress = progress
            }
            else {
                transfers[releaseId] = DownloadTransferValue(progress)
            }
        case .removed(let releaseId):
            transfers.removeValue(forKey: releaseId)
        case .reset(let keyed):
            transfers = Dictionary(
                uniqueKeysWithValues: keyed.map {
                    ($0.releaseId, DownloadTransferValue($0.progress))
                }
            )
        }
    }

    /// How far a release's download has gone; `nil` while none has arrived
    /// for it.
    public func progress(forRelease releaseId: String)
        -> BridgeDownloadTransferProgress?
    {
        transfers[releaseId]?.progress
    }
}
