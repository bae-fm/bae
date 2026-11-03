import Foundation

/// Locale-aware rendering of a storage-queue summary line from the parts bae-library
/// emits, e.g. "2 uploading · 1 failed · 3 queued". Empty when the queue is idle
/// so the band stays hidden. bae-library owns which parts appear, in what order,
/// and that a zero drops out; the UI localizes each part and joins.
public enum QueueSummary {
    /// Render core's summary parts into one " · "-joined line. The parts, their
    /// order, and the drop-if-zero rule are core's; this only localizes and joins.
    public static func line(_ parts: [BridgeCountLabel]) -> String {
        parts
            .map(\.localizedLine)
            .joined(separator: " \u{00B7} ")
    }

    /// Locale-aware transfer byte rate using the Core catalog's `/s` format.
    public static func throughputText(bytesPerSecond: UInt64) -> String {
        precondition(
            bytesPerSecond <= UInt64(Int64.max),
            "transfer throughput exceeds the platform byte formatter"
        )
        let rate = Int64(bytesPerSecond).formatted(.byteCount(style: .file))
        return String(
            format: localizedCoreString("core.outbox.throughput"),
            rate
        )
    }
}
