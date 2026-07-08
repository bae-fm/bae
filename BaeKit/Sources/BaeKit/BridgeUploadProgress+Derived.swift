import Foundation

extension BridgeUploadProgress {
    /// Total queued + active + failed (i.e. anything not yet shipped). Drives
    /// the storage row's "Uploading (N)" badge.
    public var pending: UInt32 {
        queued + active + failed
    }

    /// Byte progress as a 0...1 fraction for a determinate `ProgressView`.
    /// Cumulative over the queue burst: completed files stay in both numbers.
    /// Zero (an empty bar) when nothing is queued, which reads correctly as
    /// "not started" rather than a divide-by-zero.
    public var fraction: Double {
        guard bytesTotal > 0 else { return 0 }
        return Double(bytesDone) / Double(bytesTotal)
    }

    /// Byte progress for a queue row: "45.2 MB of 103.1 MB", cumulative over
    /// the queue burst.
    public var bytesText: String {
        String(
            format: QueueSummary.message("core.outbox.bytes_progress"),
            Int64(bytesDone).formatted(.byteCount(style: .file)),
            Int64(bytesTotal).formatted(.byteCount(style: .file))
        )
    }
}
