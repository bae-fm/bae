import Foundation

extension BridgeQueuedRelease {
    /// A queue row's title: the release's album title, or core's line for a
    /// release the library no longer holds.
    public static func titleText(_ release: BridgeQueuedRelease?) -> String {
        release?.title ?? localizedCoreString("core.queue.release_missing")
    }
}

extension BridgeDownloadOp {
    public var titleText: String { BridgeQueuedRelease.titleText(release) }

    /// A queue row's secondary line: the file count (a localized plural) and
    /// total size the entry was queued with, e.g. "12 files · 350 MB".
    /// bae-library emits the raw byte count; the UI formats it.
    public var detailText: String {
        let files = String(localized: "\(fileCount) files")
        return FactList.joined([files, totalSize.fileSizeText])
    }
}
