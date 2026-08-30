import Foundation

extension BridgeUploadProgress {
    /// The release-level state shown in Storage. A missing local source is the
    /// actionable condition; the retry count remains available as secondary
    /// queue detail.
    public var primaryActivityText: String? {
        switch issue {
        case .sourceUnavailable:
            QueueSummary.message("core.outbox.source_unavailable")
        case nil:
            activityText
        }
    }

    public var sourceUnavailablePaths: [String] {
        switch issue {
        case .sourceUnavailable(let paths): paths
        case nil: []
        }
    }

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
            case .retrying: ("core.outbox.retrying", retrying)
            case .prepared: ("core.outbox.prepared", prepared)
            case .queued: ("core.queue.queued", queued)
            case .uploaded: ("core.outbox.uploaded", uploaded)
            }
        return QueueSummary.countLabel(key, count)
    }
}

extension BridgeUploadBar {
    /// How far through its phase this bar has come, as a 0...1 fraction.
    public var fraction: Double {
        guard bytesTotal > 0 else { return 0 }
        precondition(
            bytesDone <= bytesTotal,
            "cloud upload progress cannot exceed its exact total"
        )
        return Double(bytesDone) / Double(bytesTotal)
    }

    /// "Uploading 3 MB of 221.2 MB": the phase this bar counts, then the exact
    /// bytes it counts them in. Reading the numbers off the bar itself is what
    /// keeps the text and the fill measuring the same thing.
    public var text: String {
        String(
            format: QueueSummary.message(
                bridgeUploadPhaseBytesKey(phase: phase)
            ),
            Int64(bytesDone).formatted(.byteCount(style: .file)),
            Int64(bytesTotal).formatted(.byteCount(style: .file))
        )
    }
}
