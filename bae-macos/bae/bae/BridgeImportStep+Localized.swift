import BaeKit
import Foundation

extension BridgeImportStep {
    /// The localized progress text for this step, resolved from the generated
    /// `Core` string table via the key bae-library owns. bae-library decides the step;
    /// this is the UI's locale rendering of it.
    var localizedText: String {
        let key: String
        switch self {
        case .preparing(let step):
            key = bridgePrepareStepKey(step: step)
        case .running(let phase):
            key = bridgeImportPhaseKey(phase: phase)
        }
        return localizedCoreString(key)
    }
}
