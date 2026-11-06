import Foundation

extension BridgeInvite {
    /// The invite's request waiting for the admin to approve or decline;
    /// nil while access is granted or a request awaited, and once its person
    /// is joining.
    public var requestToDecide: BridgeJoinRequest? {
        switch shown {
        case .code(_, .deciding(let request)): request
        case .granting, .code(_, .waiting), .code(_, .joining): nil
        }
    }
}
