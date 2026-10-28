import SwiftUI

/// Whether uploads are paused, which the pause control reads, and how far
/// and how fast the uploads go. The retained subscription lands the pause
/// each time it changes. What of a release waits to upload is its row's
/// `upload`, read with the row. The transfer subscriptions write each
/// release's, each file's and the total's latest values on the thread they
/// arrive on, and the native view drawing one reads it once per display
/// frame, so moving bytes run no SwiftUI update and no main-thread hop.
@MainActor
@Observable
public class OutboxStore {
    /// Whether the person paused uploads, as the retained subscription last
    /// said.
    public private(set) var uploadsPaused: Bool
    /// The latest share of each file being sent, by file.
    private nonisolated let fileShares = LatestByKey<Double>()
    /// The latest transfer of each release with files being sent.
    private nonisolated let releaseProgress =
        LatestByKey<BridgeReleaseUploadTransfer>()
    /// The files being sent together while any is.
    private nonisolated let filesBeingSent = LatestOne<BridgeUploadTransfer>()

    public init(uploadsPaused: Bool) {
        self.uploadsPaused = uploadsPaused
    }

    public func applyUploadsPaused(_ paused: Bool) {
        uploadsPaused = paused
    }

    /// Apply one change the transfer subscription delivered, on any thread.
    public nonisolated func applyTransferChange(
        _ change: BridgeOutboxTransferChange
    ) {
        switch change {
        case .updated(let releaseId, let transfer):
            releaseProgress.set(transfer, for: releaseId)
        case .removed(let releaseId):
            releaseProgress.remove(releaseId)
        case .totalUpdated(let transfer):
            setFilesBeingSent(transfer)
        case .reset(let keyed, let total):
            releaseProgress.reset(
                Dictionary(
                    uniqueKeysWithValues: keyed.map {
                        ($0.releaseId, $0.transfer)
                    }
                )
            )
            setFilesBeingSent(total)
        }
    }

    /// The files being sent together hold a bar while any is; with none they
    /// are idle.
    private nonisolated func setFilesBeingSent(_ transfer: BridgeUploadTransfer)
    {
        if transfer.bar == nil {
            filesBeingSent.clear()
        }
        else {
            filesBeingSent.set(transfer)
        }
    }

    /// Apply one change the file transfer subscription delivered, on any
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

    /// The latest of the files being sent together, while any is, for the
    /// native views that read it each display frame.
    public nonisolated var totalProgress: LatestSource<BridgeUploadTransfer> {
        filesBeingSent.source
    }

    /// The latest transfer of a release with files being sent, for the
    /// native views that read it each display frame.
    public nonisolated func uploadProgress(forRelease releaseId: String)
        -> LatestSource<BridgeReleaseUploadTransfer>
    {
        LatestSource(store: releaseProgress, key: releaseId)
    }

    /// The latest share a file being sent has moved, for the native view
    /// that reads it each display frame.
    public nonisolated func fileShare(forFile fileId: String)
        -> LatestSource<Double>
    {
        LatestSource(store: fileShares, key: fileId)
    }
}
