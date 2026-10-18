import Foundation

extension BridgeDevice {
    /// `thisDevice` — the platform's name for the device it runs on — for this
    /// device; otherwise the device's state when it no longer has access.
    public func stateLabel(thisDevice: String) -> String {
        if isSelf { return thisDevice }
        return switch state {
        case .active: ""
        case .removed: String(localized: "Removed")
        case .replaced: String(localized: "Replaced")
        }
    }
}
