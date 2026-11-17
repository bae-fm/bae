import Foundation

extension BridgeDownloadOp {
    public var titleText: String { BridgeQueuedRelease.titleText(release) }

    /// A queue row's secondary line: the file count and total size core
    /// stored when the person queued the release.
    public var detailText: String {
        BridgeQueuedRelease.filesText(
            fileCount: fileCount,
            totalSize: totalSize
        )
    }
}
