import BaeKit
import Foundation

extension BridgeFile {
    /// File size formatted for the current locale, e.g. "35 MB". bae-core emits
    /// the raw byte count; the UI formats it.
    var fileSizeText: String {
        Int64(fileSize).formatted(.byteCount(style: .file))
    }
}
