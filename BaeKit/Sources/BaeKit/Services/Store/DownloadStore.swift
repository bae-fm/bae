import SwiftUI

/// The streams of how far each downloading release, and each file a pin
/// downloads, has got. The bridge holds each latest value, and the native
/// view drawing that release or file reads it once per display frame, so
/// arriving bytes run no SwiftUI update and no main-thread hop. The queue's
/// entries are the Storage Manager inspector's, read for one release in core.
@MainActor
@Observable
public class DownloadStore {
    /// The latest share of each file being downloaded to keep whole, by file.
    public nonisolated let fileShares: LatestStream<Double>
    /// The latest progress of each downloading release.
    public nonisolated let releaseProgress:
        LatestStream<BridgeDownloadTransferProgress>

    /// A store whose streams the subscriptions fill, or, for a preview,
    /// streams holding fixed values.
    public init(
        fileShares: LatestStream<Double> = .fileShares(),
        releaseProgress: LatestStream<BridgeDownloadTransferProgress> =
            .downloads()
    ) {
        self.fileShares = fileShares
        self.releaseProgress = releaseProgress
    }

    /// The latest progress of a downloading release, for the native views
    /// that read it each display frame.
    public nonisolated func downloadProgress(forRelease releaseId: String)
        -> LatestSource<BridgeDownloadTransferProgress>
    {
        releaseProgress.source(for: releaseId)
    }

    /// The latest share a file being downloaded to keep whole has moved, for
    /// the native view that reads it each display frame.
    public nonisolated func fileShare(forFile fileId: String)
        -> LatestSource<Double>
    {
        fileShares.source(for: fileId)
    }
}
