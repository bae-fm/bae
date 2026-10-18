import Foundation

extension BridgeUploadProgress {
    /// Localized count for the activity core chose: uploading while any file
    /// moves bytes, queued otherwise. Nil while nothing is queued.
    public var activityText: String? {
        switch activity {
        case .uploading:
            QueueSummary.countLabel("core.queue.uploading", uploading)
        case .queued: QueueSummary.countLabel("core.queue.queued", queued)
        case nil: nil
        }
    }
}

extension BridgeUploadBar {
    /// How far the encrypted bytes have come, as a 0...1 fraction.
    public var fraction: Double {
        guard bytesTotal > 0 else { return 0 }
        precondition(
            bytesDone <= bytesTotal,
            "cloud upload progress cannot exceed its exact total"
        )
        return Double(bytesDone) / Double(bytesTotal)
    }

    /// "3 MB of 221.2 MB", read off the bar itself so the text and the fill
    /// measure the same thing.
    public var text: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }
}
