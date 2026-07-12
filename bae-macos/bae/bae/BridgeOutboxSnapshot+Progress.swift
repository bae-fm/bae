import BaeKit
import Foundation

extension BridgeOutboxSnapshot {
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
