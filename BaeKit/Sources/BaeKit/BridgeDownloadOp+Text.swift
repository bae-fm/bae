import Foundation

extension BridgeDownloadOp {
    public var titleText: String { BridgeQueuedRelease.titleText(release) }

    /// A queue row's secondary line: the file count (a localized plural) and
    /// total size core stored when the person queued the release, e.g.
    /// "12 files · 350 MB".
    /// bae-library emits the raw byte count; the UI formats it.
    public var detailText: String {
        let files = String(localized: "\(fileCount) files")
        return FactList.joined([files, totalSize.fileSizeText])
    }
}
