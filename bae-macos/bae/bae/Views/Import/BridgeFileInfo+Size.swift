import BaeKit
import Foundation

extension BridgeFileInfo {
    /// File size formatted for the current locale, e.g. "35 MB".
    var sizeText: String {
        Int64(size).formatted(.byteCount(style: .file))
    }
}
