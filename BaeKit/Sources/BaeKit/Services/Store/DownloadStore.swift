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
@MainActor
@Observable
public class DownloadStore {
    /// Each downloading release's progress, one value per release; the map
    /// changes only as releases start and stop downloading.
    private var transfers: [String: DownloadTransferValue] = [:]
    /// The share of each file being downloaded to keep whole its object has
    /// moved, by file, for the native views that draw each file and subscribe
    /// to them; nothing SwiftUI observes.
    private let files = KeyedValues<Double>()
    /// Each downloading release's progress, by release, for the native views
    /// that draw it; nothing SwiftUI observes.
    private let releases = KeyedValues<BridgeDownloadTransferProgress>()

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
            releases.set(progress, for: releaseId)
        case .removed(let releaseId):
            transfers.removeValue(forKey: releaseId)
            releases.remove(releaseId)
        case .reset(let keyed):
            transfers = Dictionary(
                uniqueKeysWithValues: keyed.map {
                    ($0.releaseId, DownloadTransferValue($0.progress))
                }
            )
            releases.reset(
                Dictionary(
                    uniqueKeysWithValues: keyed.map {
                        ($0.releaseId, $0.progress)
                    }
                )
            )
        }
    }

    /// Apply one change the pin file transfer subscription delivered.
    public func applyFileTransferChange(_ change: BridgeFileTransferChange) {
        switch change {
        case .updated(let fileId, let fraction):
            files.set(fraction, for: fileId)
        case .removed(let fileId):
            files.remove(fileId)
        case .reset(let keyed):
            files.reset(
                Dictionary(
                    uniqueKeysWithValues: keyed.map { ($0.fileId, $0.fraction) }
                )
            )
        }
    }

    /// The stream of the share a file being downloaded to keep whole has
    /// moved, for the native view that draws it.
    public func fileTransfer(forFile fileId: String) -> KeyedValueSource<Double>
    {
        KeyedValueSource(values: files, key: fileId)
    }

    /// The stream of a release's download progress, for the native view that
    /// draws it.
    public func releaseTransfer(forRelease releaseId: String)
        -> KeyedValueSource<BridgeDownloadTransferProgress>
    {
        KeyedValueSource(values: releases, key: releaseId)
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
