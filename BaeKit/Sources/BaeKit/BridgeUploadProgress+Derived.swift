import Foundation

extension BridgeUploadProgress {
    public var primaryActivityText: String? { activityText }
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
    public var fraction: Double {
        guard bytesTotal > 0 else { return 0 }
        precondition(bytesDone <= bytesTotal)
        return Double(bytesDone) / Double(bytesTotal)
    }
    public var text: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }
}
