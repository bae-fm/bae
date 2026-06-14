import Foundation

extension BridgeUploadProgress {
    /// True when this slice of the queue holds no queued, active, or failed work.
    var isIdle: Bool {
        queued == 0 && active == 0 && failed == 0
    }

    /// Total queued + active + failed (i.e. anything not yet shipped). Drives
    /// the storage row's "Uploading (N)" badge.
    var pending: UInt32 {
        queued + active + failed
    }

    /// Byte progress as a 0...1 fraction for a determinate `ProgressView`.
    /// Zero (an empty bar) when nothing is queued, which reads correctly as "not
    /// started" rather than a divide-by-zero.
    var fraction: Double {
        guard bytesTotal > 0 else { return 0 }
        return Double(bytesDone) / Double(bytesTotal)
    }
}
