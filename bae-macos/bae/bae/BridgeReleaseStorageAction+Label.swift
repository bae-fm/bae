import BaeKit
import Foundation

extension BridgeReleaseStorageAction {
    /// User-facing label for the action. Shared by the album-detail
    /// "Storage…" sheet and the Storage Manager row context menu so both
    /// sites read the same wording.
    var label: String {
        switch self {
        case .makeRemote: String(localized: "Move to Cloud")
        case .makeLocal: String(localized: "Make Local…")
        case .pin: String(localized: "Pin")
        case .unpin: String(localized: "Unpin")
        }
    }

    /// SF Symbol name paired with `label`.
    var systemImage: String {
        switch self {
        case .makeRemote: "arrow.down.to.line"
        case .makeLocal: "arrow.up.forward.square"
        case .pin: "pin"
        case .unpin: "pin.slash"
        }
    }
}
