import BaeKit
import Foundation

extension BridgeDownloadKind {
    /// The words the storage chip and the file mark use while waiting.
    var queuedLabel: String {
        switch self {
        case .pin: String(localized: "Queued to pin")
        case .moveToThisDevice: String(localized: "Queued to move to This Mac")
        }
    }
}
