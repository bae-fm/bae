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

/// The bytes each downloading release has moved. The transfer subscription
/// lands each release's progress into that release's own value, so bytes
/// arriving redraw only what draws that release's progress. The queue's
/// entries are the Storage Manager inspector's, read for one release in core.
@Observable
public class DownloadStore {
    /// Each downloading release's progress, one value per release; the map
    /// changes only as releases start and stop downloading.
    private var transfers: [String: DownloadTransferValue] = [:]

    public init() {}

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

    /// A release's download progress as its own observed value, for a view
    /// to hand to the one leaf that draws it: reading this follows only the
    /// release starting and stopping its download, and the leaf alone
    /// follows `.progress`. `nil` while none has arrived for it.
    public func transfer(forRelease releaseId: String)
        -> DownloadTransferValue?
    {
        transfers[releaseId]
    }
}
