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

    /// `label`, except that Unpin of a release coven is still copying files of
    /// to this Mac reads Cancel pinning: it ends the copying.
    func label(keepingHere: Bool) -> String {
        if self == .unpin && keepingHere {
            return Self.cancelPinningLabel
        }
        return label
    }

    /// SF Symbol name paired with `label`.
    var systemImage: String {
        switch self {
        case .upload: "arrow.down.to.line"
        case .detachAudio: "arrow.up.forward.square"
        case .pin: "pin"
        case .unpin: "pin.slash"
        }
    }
}
