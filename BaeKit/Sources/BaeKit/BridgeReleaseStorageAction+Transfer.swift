import Foundation

extension BridgeReleaseStorageAction {
    /// The transition's present-continuous words while it runs ("Pinning for
    /// offline"), from core's catalog.
    public var transferLabel: String {
        localizedCoreString(bridgeTransferActionKey(action: self))
    }
}
