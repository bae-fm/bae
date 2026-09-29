import Foundation

extension BridgeQueuedRelease {
    /// Total release size formatted for the current locale, e.g. "350 MB".
    /// bae-core emits the raw byte count; the UI formats it.
    public var totalSizeText: String {
        totalSize.formatted(.byteCount(style: .file))
    }

    /// A queue row's secondary line: the release's file count (a localized
    /// plural) and its total size, e.g. "12 files · 350 MB".
    public var detailText: String {
        let files = String(localized: "\(fileCount) files")
        return "\(files) · \(totalSizeText)"
    }

    /// A queue row's title: the release's album title, or core's line for a
    /// release the library no longer holds.
    public static func titleText(_ release: BridgeQueuedRelease?) -> String {
        release?.title ?? QueueSummary.message("core.queue.release_missing")
    }
}

extension BridgeDownloadOp {
    public var titleText: String { BridgeQueuedRelease.titleText(release) }
}
