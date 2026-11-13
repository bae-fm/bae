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

    /// What the choice means, under the control that picks it.
    var note: String {
        switch self {
        case .private:
            String(localized: "Only you see this playlist.")
        case .household:
            String(
                localized:
                    "Everyone in the library sees this playlist and can play it. Only you can change it."
            )
        }
    }
}
