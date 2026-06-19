import Foundation

extension BridgeUploadOp {
    /// File size formatted for the current locale, e.g. "70 MB". bae-core emits
    /// the raw byte count; the UI formats it.
    var sizeText: String {
        Int64(bytesTotal).formatted(.byteCount(style: .file))
    }
}
