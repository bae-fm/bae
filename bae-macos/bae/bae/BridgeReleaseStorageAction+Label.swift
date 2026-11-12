import BaeKit
import Foundation

extension BridgeReleaseStorageAction {
    /// User-facing label for the action. Shared by the album-detail
    /// "Storage…" sheet and the Storage Manager row context menu so both
    /// sites read the same wording.
    var label: String {
        switch self {
        case .upload: String(localized: "Move to cloud")
        case .detachAudio: String(localized: "Make local…")
        case .pin: String(localized: "Pin")
        case .unpin: String(localized: "Unpin")
        }
    }

    /// The words for ending a pin that is under way or waiting.
    static var cancelPinningLabel: String {
        String(localized: "Cancel pinning")
    }
}
