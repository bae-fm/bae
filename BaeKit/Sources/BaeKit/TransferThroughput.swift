import Foundation

/// Locale-aware rendering of how fast a transfer goes, from the bytes per
/// second core measures.
public enum TransferThroughput {
    /// The byte rate in the Core catalog's `/s` format.
    public static func text(bytesPerSecond: UInt64) -> String {
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
