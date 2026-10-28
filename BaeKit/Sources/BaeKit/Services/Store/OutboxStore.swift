import SwiftUI

/// One release's upload transfer, observed on its own: bytes moving change
/// only this value, so only a view that draws it redraws.
@Observable
public final class ReleaseUploadTransferValue {
    public internal(set) var transfer: BridgeReleaseUploadTransfer

    init(_ transfer: BridgeReleaseUploadTransfer) {
        self.transfer = transfer
    }
}

/// The files being sent together, observed on their own.
@Observable
public final class UploadTransferValue {
    public internal(set) var transfer: BridgeUploadTransfer

    init(_ transfer: BridgeUploadTransfer) {
        self.transfer = transfer
    }
}

/// Whether uploads are paused, which the pause control reads, and how far
/// and how fast the uploads go. The retained subscription lands the pause
/// each time it changes. What of a release waits to upload is its row's
/// `upload`, read with the row. The transfer subscription lands each
/// release's bytes and rate into that release's own value, so a move redraws
/// only what draws that release. Views read both at the leaf.
@Observable
public class OutboxStore {
    /// Whether the person paused uploads, as the retained subscription last
    /// said.
    public private(set) var uploadsPaused: Bool
    /// Each release with files being sent, its transfer, by release id; the
    /// map changes only as releases come and go.
    private var releases: [String: ReleaseUploadTransferValue] = [:]
    /// The files being sent together; `nil` until the transfers first arrive.
    private var totalTransfer: UploadTransferValue?
    /// How far each file being sent has got, by file, for a surface that
    /// draws each file and subscribes to them.
    private let files = FileTransfers()

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
            if let value = releases[releaseId] {
                value.transfer = transfer
            }
            else {
                releases[releaseId] = ReleaseUploadTransferValue(transfer)
            }
        case .removed(let releaseId):
            releases.removeValue(forKey: releaseId)
        case .totalUpdated(let transfer):
            setTotal(transfer)
        case .reset(let keyed, let total):
            releases = Dictionary(
                uniqueKeysWithValues: keyed.map {
                    ($0.releaseId, ReleaseUploadTransferValue($0.transfer))
                }
            )
            setTotal(total)
        }
    }

    private func setTotal(_ transfer: BridgeUploadTransfer) {
        if let totalTransfer {
            totalTransfer.transfer = transfer
        }
        else {
            totalTransfer = UploadTransferValue(transfer)
        }
    }

    /// Apply one change the file transfer subscription delivered.
    public func applyFileTransferChange(_ change: BridgeFileTransferChange) {
        files.apply(change)
    }

    /// How far a release's upload has got, of its whole size, and how fast
    /// its files being sent go together; `nil` while none of them is being
    /// sent.
    public func transfer(forRelease releaseId: String)
        -> BridgeReleaseUploadTransfer?
    {
        releases[releaseId]?.transfer
    }

    /// The share of a file being sent its object has moved; `nil` while
    /// nothing of it has.
    public func fileTransfer(forFile fileId: String) -> Double? {
        files.fraction(forFile: fileId)
    }

    /// How far the files being sent have gone together; `nil` until the
    /// transfers first arrive, or while no file is being sent.
    public var totalBar: BridgeUploadBar? {
        totalTransfer?.transfer.bar
    }

    /// The upload rate of every file being sent. A zero reading stays absent
    /// instead of presenting a stalled speed.
    public var throughputText: String? {
        guard let throughputBps = totalTransfer?.transfer.throughputBps,
            throughputBps > 0
        else { return nil }
        return QueueSummary.throughputText(bytesPerSecond: throughputBps)
    }

    /// A release's upload as a Storage surface draws it: what of it waits,
    /// read with its row, and how far and how fast its files go, read from
    /// its transfer. `nil` while none of its files waits.
    public func uploadObservation(of release: ReleaseSummary)
        -> StorageUploadObservation?
    {
        release.upload.map {
            StorageUploadObservation(
                upload: $0,
                bar: transfer(forRelease: release.id)?.bar,
                throughputBps: transfer(forRelease: release.id)?.throughputBps
            )
        }
    }

}

/// A release's files in the upload queue as a Storage surface draws them:
/// how many wait and are being sent, how far the release has got, and how
/// fast its files go.
public struct StorageUploadObservation: Equatable {
    public let upload: BridgeReleaseUpload
    /// Of the release's whole size, the bytes stored and moved; `nil` while
    /// none of its files is being sent.
    public let bar: BridgeUploadBar?
    /// The release's upload rate; `nil` while none of its files is being
    /// sent.
    public let throughputBps: UInt64?

    public init(
        upload: BridgeReleaseUpload,
        bar: BridgeUploadBar?,
        throughputBps: UInt64?
    ) {
        self.upload = upload
        self.bar = bar
        self.throughputBps = throughputBps
    }

    public var progress: BridgeUploadProgress { upload.progress }

    public var transitionStatusText: String {
        [transitionPhaseText, progressDetailText]
            .compactMap { $0 }
            .joined(separator: " \u{00B7} ")
    }

    /// The release's phase alone, for a line that puts the bar between the
    /// phase and its count. It names the release's phase, not how many of
    /// its files are in it.
    public var transitionPhaseText: String {
        progress.heldActivity.phaseText
    }

    /// The bytes the release holds of its size, while any file is being
    /// sent.
    public var progressDetailText: String? {
        bar?.text
    }

    /// The bar the upload draws: exact progress while files are being sent,
    /// and none for files that are only waiting: nothing is moving them, and
    /// with no storage connected nothing will until one is.
    public var progressFraction: Double? {
        bar?.fraction
    }

    /// The release's current upload rate. A zero reading stays absent
    /// instead of presenting a stalled speed.
    public var throughputText: String? {
        guard let throughputBps, throughputBps > 0 else { return nil }
        return QueueSummary.throughputText(bytesPerSecond: throughputBps)
    }
}
