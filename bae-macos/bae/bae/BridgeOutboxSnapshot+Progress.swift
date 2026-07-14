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

    /// Estimated time remaining, e.g. "2:14 remaining". Empty when not
    /// computable.
    ///
    /// The duration reads as a clock, through core's projection — the same form
    /// Windows renders and the same one track times use throughout the app.
    /// `DurationUnits` (the album-total shape) is minute-granular, so a
    /// sub-minute ETA would read "0 min"; the clock keeps the seconds an ETA
    /// needs. The app renders the clock's digits; core owns its shape.
    var etaText: String {
        guard let eta = etaSeconds else { return "" }
        let duration = DurationClock.text(Int64(eta) * 1000)
        return String(format: QueueSummary.message("core.outbox.eta"), duration)
    }
}
