import Foundation

extension BridgeDownloadOp {
    public var titleText: String { BridgeQueuedRelease.titleText(release) }

    /// A queue row's secondary line: the file count and total size that the
    /// queue stored for the entry.
    public var detailText: String {
        BridgeQueuedRelease.filesText(
            fileCount: fileCount,
            totalSize: totalSize
        )
    }
}
