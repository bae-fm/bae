import SwiftUI

/// One upload rate, observed on its own: a part the provider confirms
/// changes only this value, so only a view that draws it redraws.
@Observable
public final class UploadRateValue {
    /// Bytes per second.
    public internal(set) var throughputBps: UInt64

    init(_ throughputBps: UInt64) {
        self.throughputBps = throughputBps
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

/// Whether uploads are paused, which the pause control reads, and how fast
/// the uploads go. The retained subscription lands the pause each time it
/// changes. What of a release waits to upload, and the bytes its files being
/// sent have moved, are its row's `upload`, read with the row. The transfer
/// subscription lands each file's and release's rate into that key's own
/// value, so a rate change redraws only what draws that key. Views read both
/// at the leaf.
@Observable
public class OutboxStore {
    /// Whether the person paused uploads, as the retained subscription last
    /// said.
    public private(set) var uploadsPaused: Bool
    /// Each file being sent and each release with files being sent, its rate;
    /// the map changes only as keys come and go.
    private var rates: [BridgeUploadTransferKey: UploadRateValue] = [:]
    /// The files being sent together; `nil` until the transfers first arrive.
    private var totalTransfer: UploadTransferValue?

    public init(uploadsPaused: Bool) {
        self.uploadsPaused = uploadsPaused
    }

    public func applyUploadsPaused(_ paused: Bool) {
        uploadsPaused = paused
    }

    /// Apply one change the transfer subscription delivered.
    public func applyTransferChange(_ change: BridgeOutboxTransferChange) {
        switch change {
        case .updated(let key, let throughputBps):
            if let value = rates[key] {
                value.throughputBps = throughputBps
            }
            else {
                rates[key] = UploadRateValue(throughputBps)
            }
        case .removed(let key):
            rates.removeValue(forKey: key)
        case .totalUpdated(let transfer):
            setTotal(transfer)
        case .reset(let keyed, let total):
            rates = Dictionary(
                uniqueKeysWithValues: keyed.map {
                    ($0.key, UploadRateValue($0.throughputBps))
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

    /// How fast a release's files being sent go together; `nil` while none
    /// of them is being sent.
    public func throughput(forRelease releaseId: String) -> UInt64? {
        rates[.release(releaseId: releaseId)]?.throughputBps
    }

    /// How fast one file being sent goes; `nil` while it is not being sent.
    public func throughput(forFile fileId: String) -> UInt64? {
        rates[.file(fileId: fileId)]?.throughputBps
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
    /// read with its row, and how fast its files go, read from its transfer.
    /// `nil` while none of its files waits.
    public func uploadObservation(of release: ReleaseSummary)
        -> StorageUploadObservation?
    {
        release.upload.map {
            StorageUploadObservation(
                upload: $0,
                throughputBps: throughput(forRelease: release.id)
            )
        }
    }

}

/// A release's files in the upload queue as a Storage surface draws them:
/// how many wait and are being sent, the bytes of those being sent, and how
/// fast they go.
public struct StorageUploadObservation: Equatable {
    public let upload: BridgeReleaseUpload
    /// The release's upload rate; `nil` while none of its files is being
    /// sent.
    public let throughputBps: UInt64?

    public init(upload: BridgeReleaseUpload, throughputBps: UInt64?) {
        self.upload = upload
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

    /// The bytes the files being sent have moved, while any is.
    public var progressDetailText: String? {
        upload.bar?.text
    }

    /// The bar the upload draws: exact progress while files are being sent,
    /// and none for files that are only waiting: nothing is moving them, and
    /// with no storage connected nothing will until one is.
    public var progressFraction: Double? {
        upload.bar?.fraction
    }

    /// The release's current upload rate. A zero reading stays absent
    /// instead of presenting a stalled speed.
    public var throughputText: String? {
        guard let throughputBps, throughputBps > 0 else { return nil }
        return QueueSummary.throughputText(bytesPerSecond: throughputBps)
    }
}
