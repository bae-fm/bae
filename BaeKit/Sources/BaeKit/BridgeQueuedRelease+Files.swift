import Foundation

extension BridgeQueuedRelease {
    /// A queue row's file count (a localized plural) and total size, as the
    /// entry was queued with them: "12 files · 350 MB". bae-library emits
    /// the raw byte count; the UI formats it.
    public static func filesText(fileCount: UInt64, totalSize: UInt64)
        -> String
    {
        let files = String(localized: "\(fileCount) files")
        return FactList.joined([files, totalSize.fileSizeText])
    }
}
