import SwiftUI

/// How far each downloading release, and each file a pin downloads, has got.
/// The transfer subscriptions write each latest value on the thread they
/// arrive on, and the native view drawing that release or file reads it
/// once per display frame, so arriving bytes run no SwiftUI update and no
/// main-thread hop. The queue's entries are the Storage Manager inspector's,
/// read for one release in core.
@MainActor
@Observable
public class DownloadStore {
    /// The latest share of each file being downloaded to keep whole, by file.
    private nonisolated let fileShares = LatestByKey<Double>()
    /// The latest progress of each downloading release.
    private nonisolated let releaseProgress =
        LatestByKey<BridgeDownloadTransferProgress>()

    public init() {}

    /// Apply one change the transfer subscription delivered, on any thread.
    public nonisolated func applyTransferChange(
        _ change: BridgeDownloadTransferChange
    ) {
        switch change {
        case .updated(let releaseId, let progress):
            releaseProgress.set(progress, for: releaseId)
        case .removed(let releaseId):
            releaseProgress.remove(releaseId)
        case .reset(let keyed):
            releaseProgress.reset(
                Dictionary(
                    uniqueKeysWithValues: keyed.map {
                        ($0.releaseId, $0.progress)
                    }
                )
            )
        }
    }

    /// Apply one change the pin file transfer subscription delivered, on any
    /// thread.
    public nonisolated func applyFileTransferChange(
        _ change: BridgeFileTransferChange
    ) {
        switch change {
        case .updated(let fileId, let fraction):
            fileShares.set(fraction, for: fileId)
        case .removed(let fileId):
            fileShares.remove(fileId)
        case .reset(let keyed):
            fileShares.reset(
                Dictionary(
                    uniqueKeysWithValues: keyed.map { ($0.fileId, $0.fraction) }
                )
            )
        }
    }

    /// The latest progress of a downloading release, for the native views
    /// that read it each display frame.
    public nonisolated func downloadProgress(forRelease releaseId: String)
        -> LatestSource<BridgeDownloadTransferProgress>
    {
        LatestSource(store: releaseProgress, key: releaseId)
    }

    /// The latest share a file being downloaded to keep whole has moved, for
    /// the native view that reads it each display frame.
    public nonisolated func fileShare(forFile fileId: String)
        -> LatestSource<Double>
    {
        LatestSource(store: fileShares, key: fileId)
    }
}
