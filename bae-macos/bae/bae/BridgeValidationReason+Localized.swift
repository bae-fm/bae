import BaeKit
import Foundation

extension BridgeValidationReason {
    /// The localized reason text, resolved from the generated `Core` string
    /// table via the key bae-library owns (`bridgeValidationReasonKey`). bae-library
    /// decides which reason; this is the UI's locale rendering of it.
    var localizedMessage: String {
        localizedCoreString(bridgeValidationReasonKey(reason: self))
    }
}
