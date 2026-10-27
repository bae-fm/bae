import SwiftUI
import os.log

private let logger = Logger.bae("OutboxStore")

/// One upload transfer, observed on its own: a part the provider confirms
/// changes only this value, so only a view that draws it redraws.
@Observable
public final class UploadTransferValue {
    public internal(set) var transfer: BridgeUploadTransfer

    init(_ transfer: BridgeUploadTransfer) {
        self.transfer = transfer
    }
}

/// Mirror of core's cloud upload queue summary, which the pause control
/// reads, and of the queue's transfers. The retained outbox subscription
/// lands the summary each time the queue changes. What of a release waits to
/// upload is its row's `upload`, read with the row. The transfer subscription
/// lands each file's and release's rate into that key's own value, so a rate
/// change redraws only what draws that key. Views read both at the leaf.
@Observable
public class OutboxStore {
    public var snapshot: BridgeOutboxSnapshot
    /// Each queued file's and uploading release's transfer, one value per
    /// key; the map changes only as keys come and go.
    private var transfers: [BridgeUploadTransferKey: UploadTransferValue] = [:]
    /// The whole queue's transfer; `nil` until the transfers first arrive.
    private var totalTransfer: UploadTransferValue?

    public init(snapshot: BridgeOutboxSnapshot) {
        self.snapshot = snapshot
    }

    public func applySnapshot(_ snapshot: BridgeOutboxSnapshot) {
        guard snapshot.revision >= self.snapshot.revision else {
            logger.debug(
                "dropping outbox snapshot at revision \(snapshot.revision); revision \(self.snapshot.revision) is already applied"
            )
            return
        }
        self.snapshot = snapshot
    }

    /// Apply one change the transfer subscription delivered.
    public func applyTransferChange(_ change: BridgeOutboxTransferChange) {
        switch change {
        case .updated(let key, let transfer):
            if let value = transfers[key] {
                value.transfer = transfer
            }
            else {
                transfers[key] = UploadTransferValue(transfer)
            }
        case .removed(let key):
            transfers.removeValue(forKey: key)
        case .totalUpdated(let transfer):
            setTotal(transfer)
        case .reset(let keyed, let total):
            transfers = Dictionary(
                uniqueKeysWithValues: keyed.map {
                    ($0.key, UploadTransferValue($0.transfer))
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

    /// How far a release's uploading files have gone together, and how fast;
    /// `nil` while none has arrived for it.
    public func transfer(forRelease releaseId: String) -> BridgeUploadTransfer?
    {
        transfers[.release(releaseId: releaseId)]?.transfer
    }

    /// How far one queued file has gone, and how fast; `nil` while none has
    /// arrived for it.
    public func transfer(forFile fileId: String) -> BridgeUploadTransfer? {
        transfers[.file(fileId: fileId)]?.transfer
    }

    /// How far the files being sent have gone together; `nil` until the
    /// transfers first arrive, or while no file is sent in parts.
    public var totalBar: BridgeUploadBar? {
        totalTransfer?.transfer.bar
    }

    /// The whole queue's upload rate. A zero reading stays absent instead of
    /// presenting a stalled speed.
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
                throughputBps: transfer(forRelease: release.id)?.throughputBps
            )
        }
    }

    /// The idle queue used before the required initial snapshot arrives and by
    /// previews/tests. A failed initial read fails library opening instead of
    /// presenting this value as authoritative queue state.
    public static var emptySnapshot: BridgeOutboxSnapshot {
        BridgeOutboxSnapshot(
            revision: 0,
            perRelease: [:],
            pauseState: .running,
        )
    }
}

/// A release's files in the upload queue as a Storage surface draws them:
/// how many wait and are being sent, the bytes of those being sent, and how
/// fast they go.
public struct StorageUploadObservation: Equatable {
    public let upload: BridgeReleaseUpload
    /// The release's upload rate; `nil` until its transfer arrives.
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
