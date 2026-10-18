import Foundation

extension BridgeMemberRole {
    public var label: String {
        switch self {
        case .admin: String(localized: "Admin")
        case .member: String(localized: "Member")
        }
    }
}
