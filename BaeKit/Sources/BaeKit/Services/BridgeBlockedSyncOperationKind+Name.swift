import Foundation

extension BridgeBlockedSyncOperationKind {
    /// What kind of work stopped, in the reader's own words. The description and
    /// error beside it are coven's vocabulary and stay untranslated — this line
    /// is the part of a blocked operation a person can act on.
    public var localizedName: String {
        switch self {
        case .write:
            String(localized: "Library change")
        case .circleOperation:
            String(localized: "Sharing change")
        case .reclaim:
            String(localized: "Cloud cleanup")
        }
    }
}
