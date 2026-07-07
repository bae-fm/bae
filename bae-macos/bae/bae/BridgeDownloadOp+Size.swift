import Foundation

extension BridgeDownloadOp {
    /// Total release size formatted for the current locale, e.g. "350 MB".
    /// bae-core emits the raw byte count; the UI formats it.
    var totalSizeText: String {
        totalSize.formatted(.byteCount(style: .file))
    }

    /// A download-queue row's secondary line: the release's file count (a
    /// localized plural) and its total size, e.g. "12 files · 350 MB".
    var detailText: String {
        let files = String(localized: "\(fileCount) files")
        return "\(files) · \(totalSizeText)"
    }
}
