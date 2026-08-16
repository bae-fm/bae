import Foundation

extension BridgeUploadProgress {
    /// Localized count for the dominant durable or transient phase. Core owns
    /// the dominant phase; this resolves its catalog key and matching count.
    public var activityText: String? {
        guard let activity else { return nil }
        let (key, count): (String, UInt32) =
            switch activity {
            case .cancelling: ("core.outbox.cancelling", cancelling)
            case .publishing: ("core.outbox.publishing", publishing)
            case .uploading: ("core.queue.uploading", uploading)
            case .preparing: ("core.outbox.preparing", preparing)
            case .retrying: ("core.queue.failed", failed)
            case .prepared: ("core.outbox.prepared", prepared)
            case .queued: ("core.queue.queued", queued)
            case .uploaded: ("core.outbox.uploaded", uploaded)
            }
        return QueueSummary.countLabel(key, count)
    }

    /// Source-preparation + provider-upload work as a 0...1 fraction.
    public var fraction: Double {
        guard workTotal > 0 else { return 0 }
        precondition(
            workDone <= workTotal,
            "cloud upload work cannot exceed its exact total"
        )
        return Double(workDone) / Double(workTotal)
    }

    /// Exact byte progress for the dominant active stage. Preparation measures
    /// plaintext consumed; upload measures encrypted bytes sent.
    public var stageBytesText: String? {
        let values: (UInt64, UInt64)? =
            switch activity {
            case .preparing:
                (preparationBytesDone, preparationBytesTotal)
            case .uploading:
                uploadBytesTotalComplete
                    ? (uploadBytesDone, uploadBytesTotal)
                    : nil
            default:
                nil
            }
        guard let (done, total) = values, total > 0 else { return nil }
        return String(
            format: QueueSummary.message("core.outbox.bytes_progress"),
            Int64(done).formatted(.byteCount(style: .file)),
            Int64(total).formatted(.byteCount(style: .file))
        )
    }
}
