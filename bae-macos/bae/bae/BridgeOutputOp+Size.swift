import BaeKit
import Foundation

extension BridgeOutputOp {
    /// Total release size formatted for the current locale, e.g. "350 MB".
    /// bae-core emits the raw byte count; the UI formats it.
    var totalSizeText: String {
        totalSize.formatted(.byteCount(style: .file))
    }
}
