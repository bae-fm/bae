import Foundation

/// Locale-aware rendering of a storage-queue summary line from the parts bae-core
/// emits, e.g. "2 uploading · 1 failed · 3 queued · 1 pending delete". Empty when
/// the queue is idle so the band stays hidden. bae-core owns which parts appear,
/// in what order, and that a zero drops out; the UI localizes each part and joins.
public enum QueueSummary {
    /// A raw `Core`-table format string for a storage-queue message, e.g. the
    /// "{done} of {total}" byte-progress line. Callers apply their own
    /// arguments.
    public static func message(_ key: String) -> String {
        NSLocalizedString(key, tableName: "Core", bundle: .main, comment: "")
    }

    /// Localized "{count} <state>" — or the pluralized pending-delete line —
    /// resolved against the `Core` table with a locale-formatted count.
    public static func countLabel(_ key: String, _ count: UInt32) -> String {
        String.localizedStringWithFormat(message(key), Int(count))
    }

    /// Render core's summary parts into one " · "-joined line. The parts, their
    /// order, and the drop-if-zero rule are core's; this only localizes and joins.
    public static func line(_ parts: [BridgeCountLabel]) -> String {
        parts
            .map { countLabel($0.key, $0.count) }
            .joined(separator: " \u{00B7} ")
    }
}

extension BridgeOutboxSnapshot {
    public var summaryText: String { QueueSummary.line(summaryParts) }

    /// The user's pause target. `Pausing` still counts as requested even though
    /// the provider write already in progress has not finished yet.
    public var pauseRequested: Bool { pauseState != .running }
}

extension BridgeDownloadSnapshot {
    public var summaryText: String { QueueSummary.line(summaryParts) }
}

// The export queue is desktop-gated, so `BridgeOutputSnapshot` isn't generated
// for iOS. The outbox and download snapshots exist on both platforms.
#if !os(iOS)
    extension BridgeOutputSnapshot {
        public var summaryText: String { QueueSummary.line(summaryParts) }
    }
#endif
