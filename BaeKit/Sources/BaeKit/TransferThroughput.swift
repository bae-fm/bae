import Foundation

/// Locale-aware rendering of how fast a transfer goes, from the bytes per
/// second core measures.
public enum TransferThroughput {
    /// The byte rate in the Core catalog's `/s` format.
    public static func text(bytesPerSecond: UInt64) -> String {
        String(
            format: localizedCoreString("core.outbox.throughput"),
            bytesPerSecond.fileSizeText
        )
    }
}
