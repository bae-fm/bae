import BaeKit
import Foundation

extension BridgeCueFlacPair {
    /// CUE file size formatted for the current locale, e.g. "1 KB".
    var cueSizeText: String {
        Int64(cueSize).formatted(.byteCount(style: .file))
    }

    /// Combined size formatted for the current locale, e.g. "340 MB".
    var totalSizeText: String {
        Int64(totalSize).formatted(.byteCount(style: .file))
    }
}
