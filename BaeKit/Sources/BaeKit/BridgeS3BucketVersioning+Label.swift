import Foundation

extension BridgeS3BucketVersioning {
    /// The bucket's versioning state, as setup's detected settings show it.
    public var label: String {
        switch self {
        case .disabled: String(localized: "Off")
        case .enabled: String(localized: "On")
        case .suspended: String(localized: "Suspended")
        case .unavailable: String(localized: "Unavailable")
        }
    }
}
