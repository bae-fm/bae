import BaeKit
import Foundation

extension BridgeOutboxSnapshot {
    /// Bytes uploaded of the burst's total, e.g. "1.2 GB of 3.4 GB".
    /// Cumulative: completed files stay in both numbers, so the text climbs
    /// monotonically over the whole queue. Empty when no upload work exists.
    var transferProgressText: String {
        guard total.bytesTotal > 0 else { return "" }
        return String(
            format: QueueSummary.message("core.outbox.bytes_progress"),
            Int64(total.bytesDone).formatted(.byteCount(style: .file)),
            Int64(total.bytesTotal).formatted(.byteCount(style: .file))
        )
    }

    /// The burst's progress as a 0...1 fraction for the master progress bar.
    /// Zero (an empty bar) when no upload work exists.
    var transferFraction: Double {
        guard total.bytesTotal > 0 else { return 0 }
        return Double(total.bytesDone) / Double(total.bytesTotal)
    }

    /// Upload throughput, e.g. "5.2 MB/s". Empty when idle.
    var throughputText: String {
        guard throughputBps > 0 else { return "" }
        return String(
            format: QueueSummary.message("core.outbox.throughput"),
            Int64(throughputBps).formatted(.byteCount(style: .file))
        )
    }

    /// Estimated time remaining, e.g. "2 min 14 sec remaining". Empty when not
    /// computable.
    var etaText: String {
        guard let eta = etaSeconds else { return "" }
        let duration = Duration.seconds(Int64(eta))
            .formatted(
                .units(
                    allowed: [.hours, .minutes, .seconds],
                    width: .abbreviated
                )
            )
        return String(format: QueueSummary.message("core.outbox.eta"), duration)
    }
}
