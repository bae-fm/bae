import Foundation

extension BridgeReleaseStorageAction {
    /// User-facing label for the action. Shared by the album-detail
    /// "Storage…" sheet and the Storage Manager row context menu so both
    /// sites read the same wording.
    var label: String {
        switch self {
        case .manage: "Move into library"
        case .unmanage: "Move out of library..."
        case .pin: "Pin for offline"
        case .unpin: "Remove local copy"
        }
    }

    /// SF Symbol name paired with `label`.
    var systemImage: String {
        switch self {
        case .manage: "arrow.down.to.line"
        case .unmanage: "arrow.up.forward.square"
        case .pin: "pin"
        case .unpin: "pin.slash"
        }
    }
}
