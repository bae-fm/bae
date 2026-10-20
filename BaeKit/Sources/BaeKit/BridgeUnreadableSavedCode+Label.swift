import Foundation

extension BridgeUnreadableSavedCode {
    /// Why a saved recovery code can't be used, as its row in the saved-code
    /// list says it.
    public var label: String {
        switch self {
        case .format: String(localized: "Saved in a format bae can't read")
        case .otherLibrary: String(localized: "Saved for a different library")
        case .ambiguous: String(localized: "Saved more than once")
        }
    }
}
