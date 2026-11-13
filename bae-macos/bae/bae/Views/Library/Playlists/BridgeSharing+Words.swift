import BaeKit
import Foundation

extension BridgeSharing {
    /// Who sees a playlist, in the words the history setting uses.
    var label: String {
        switch self {
        case .private: String(localized: "Only me")
        case .household: String(localized: "Everyone")
        }
    }
}
