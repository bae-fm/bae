import SwiftUI

/// Whether uploads are paused, which the pause control reads, and the
/// streams of how far and how fast the uploads go. The retained subscription
/// lands the pause each time it changes. What of a release waits to upload is
/// its row's `upload`, read with the row. The bridge holds each release's,
/// each file's and the total's latest values, and the native view drawing
/// one reads it once per display frame, so moving bytes run no SwiftUI
/// update and no main-thread hop.
@MainActor
@Observable
public class OutboxStore {
    /// Whether the person paused uploads, as the retained subscription last
    /// said.
    public private(set) var uploadsPaused: Bool
    /// The latest share of each file being sent, by file.
    public nonisolated let fileShares: LatestStream<Double>
    /// The latest transfer of each release with files being sent.
    public nonisolated let releaseProgress:
        LatestStream<BridgeReleaseUploadTransfer>
    /// The files being sent together while any is.
    public nonisolated let filesBeingSent: LatestStream<BridgeUploadTransfer>

    /// A store whose streams the subscriptions fill, or, for a preview,
    /// streams holding fixed values.
    public init(
        uploadsPaused: Bool,
        fileShares: LatestStream<Double> = .fileShares(),
        releaseProgress: LatestStream<BridgeReleaseUploadTransfer> =
            .releaseUploads(),
        filesBeingSent: LatestStream<BridgeUploadTransfer> = .uploadTotal()
    ) {
        self.uploadsPaused = uploadsPaused
        self.fileShares = fileShares
        self.releaseProgress = releaseProgress
        self.filesBeingSent = filesBeingSent
    }

    public func applyUploadsPaused(_ paused: Bool) {
        uploadsPaused = paused
    }

    /// The latest of the files being sent together, while any is, for the
    /// native views that read it each display frame.
    public nonisolated var totalProgress: LatestSource<BridgeUploadTransfer> {
        filesBeingSent.only
    }

    /// The latest transfer of a release with files being sent, for the
    /// native views that read it each display frame.
    public nonisolated func uploadProgress(forRelease releaseId: String)
        -> LatestSource<BridgeReleaseUploadTransfer>
    {
        releaseProgress.source(for: releaseId)
    }

    /// The latest share a file being sent has moved, for the native view
    /// that reads it each display frame.
    public nonisolated func fileShare(forFile fileId: String)
        -> LatestSource<Double>
    {
        fileShares.source(for: fileId)
    }
}
