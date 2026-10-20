import Foundation

extension BridgeFormerDeviceState {
    /// How the device lost access, shown beside its name.
    public var label: String {
        switch self {
        case .removed: String(localized: "Removed")
        case .replaced: String(localized: "Replaced")
        }
    }
}
