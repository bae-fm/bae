import Foundation

/// Locale-aware composition of a storage-queue summary line from the typed
/// counts bae-core emits, e.g. "2 uploading · 1 failed · 3 queued · 1 pending
/// delete". Empty when the queue is idle so the band stays hidden. bae-core owns
/// the counts; the UI composes and localizes them.
public enum QueueSummary {
    /// Localized "{count} <state>" — or the pluralized pending-delete line —
    /// resolved against the `Core` table with a locale-formatted count.
    public static func countLabel(_ key: String, _ count: UInt32) -> String {
        String.localizedStringWithFormat(
            NSLocalizedString(
                key,
                tableName: "Core",
                bundle: .main,
                comment: ""
            ),
            Int(count)
        )
    }
}

extension BridgeOutboxSnapshot {
    public var summaryText: String {
        var parts: [String] = []
        for (key, count) in [
            ("core.queue.uploading", total.active),
            ("core.queue.failed", total.failed),
            ("core.queue.queued", total.queued),
        ] where count > 0 {
            parts.append(QueueSummary.countLabel(key, count))
        }
        if pendingDeletes > 0 {
            parts.append(
                QueueSummary.countLabel(
                    "core.outbox.pending_deletes",
                    pendingDeletes
                )
            )
        }
        return parts.joined(separator: " · ")
    }
}

extension BridgeDownloadSnapshot {
    public var summaryText: String {
        var parts: [String] = []
        for (key, count) in [
            ("core.queue.downloading", total.active),
            ("core.queue.failed", total.failed),
            ("core.queue.queued", total.queued),
        ] where count > 0 {
            parts.append(QueueSummary.countLabel(key, count))
        }
        return parts.joined(separator: " · ")
    }
}

// The export queue is desktop-gated, so `BridgeExportSnapshot` isn't generated
// for iOS. The outbox and download snapshots exist on both platforms.
#if !os(iOS)
    extension BridgeExportSnapshot {
        public var summaryText: String {
            var parts: [String] = []
            for (key, count) in [
                ("core.queue.exporting", total.active),
                ("core.queue.failed", total.failed),
                ("core.queue.queued", total.queued),
            ] where count > 0 {
                parts.append(QueueSummary.countLabel(key, count))
            }
            return parts.joined(separator: " · ")
        }
    }
#endif
