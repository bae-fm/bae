import Foundation

extension BridgeReleaseStorageAction {
    /// Present-continuous progress verb ("Pinning for offline"), localized
    /// against the generated `Core` table. bae-core decides the action; this is
    /// the UI's locale rendering of it.
    var transferProgressVerb: String {
        NSLocalizedString(
            bridgeTransferActionKey(action: self),
            tableName: "Core",
            bundle: .main,
            comment: ""
        )
    }
}
