import Foundation

extension BridgeValidationReason {
    /// The localized reason text, resolved from the generated `Core` string
    /// table via the key bae-core owns (`bridgeValidationReasonKey`). bae-core
    /// decides which reason; this is the UI's locale rendering of it.
    var localizedMessage: String {
        NSLocalizedString(
            bridgeValidationReasonKey(reason: self),
            tableName: "Core",
            bundle: .main,
            comment: ""
        )
    }
}
