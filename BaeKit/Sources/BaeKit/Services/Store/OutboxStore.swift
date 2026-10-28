import SwiftUI

/// Whether uploads are paused, which the pause control reads, and how far
/// and how fast the uploads go. The retained subscription lands the pause
/// each time it changes. What of a release waits to upload is its row's
/// `upload`, read with the row. The transfer subscriptions land each
/// release's, each file's and the total's bytes in stores of values nothing
/// observes, and the native view drawing one sets itself from it, so moving
/// bytes run no SwiftUI update.
@MainActor
@Observable
public class OutboxStore {
    /// Whether the person paused uploads, as the retained subscription last
    /// said.
    public private(set) var uploadsPaused: Bool
    /// The latest share of each file in flight, by file, which the file's
    /// native view reads each display frame.
    private nonisolated let fileShares = LatestByKey<Double>()
    /// Each release with files being sent, its transfer, by release.
    private let releaseTransfers = KeyedValues<BridgeReleaseUploadTransfer>()
    /// The files being sent together, for the native views that draw them.
    public let totalTransferStream = LatestValue<BridgeUploadTransfer>()

    public init(uploadsPaused: Bool) {
        self.uploadsPaused = uploadsPaused
    }

    public func applyUploadsPaused(_ paused: Bool) {
        uploadsPaused = paused
    }

    /// Apply one change the transfer subscription delivered.
    public func applyTransferChange(_ change: BridgeOutboxTransferChange) {
        switch change {
        case .updated(let releaseId, let transfer):
            releaseTransfers.set(transfer, for: releaseId)
        case .removed(let releaseId):
            releaseTransfers.remove(releaseId)
        case .totalUpdated(let transfer):
            totalTransferStream.set(transfer)
        case .reset(let keyed, let total):
            releaseTransfers.reset(
                Dictionary(
                    uniqueKeysWithValues: keyed.map {
                        ($0.releaseId, $0.transfer)
                    }
                )
            )
            totalTransferStream.set(total)
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
            let all = Dictionary(
                uniqueKeysWithValues: keyed.map { ($0.fileId, $0.fraction) }
            )
            fileShares.reset(all)
        }
    }

    /// The stream of a release's upload transfer, for the native view that
    /// draws it.
    public func releaseTransfer(forRelease releaseId: String)
        -> KeyedValueSource<BridgeReleaseUploadTransfer>
    {
        KeyedValueSource(values: releaseTransfers, key: releaseId)
    }

    /// The latest share a file in flight has moved, for the native view that
    /// reads it each display frame.
    public nonisolated func fileShare(forFile fileId: String)
        -> LatestSource<Double>
    {
        LatestSource(store: fileShares, key: fileId)
    }
}
