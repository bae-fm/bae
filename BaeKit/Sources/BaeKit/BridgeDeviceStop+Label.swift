import Foundation

extension BridgeDeviceStop {
    /// Why this device stopped for good, ending a change it asked for before
    /// the change was made, as a sentence.
    public var label: String {
        switch self {
        case .removed:
            String(localized: "This device was removed from the library.")
        case .replaced:
            String(
                localized:
                    "A restore replaced this device’s copy of the library."
            )
        case .copied:
            String(
                localized:
                    "Another copy of this device’s library is syncing as this device."
            )
        case .memberKeysReplaced:
            String(
                localized: "Your keys were replaced on another of your devices."
            )
        }
    }
}
