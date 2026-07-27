import BaeKit
import Foundation

extension BridgeIdentifyPhase {
    /// The still-identifying row's sub-line. UI-owned copy, not a `Core`
    /// catalog key: the phase names three different kinds of "no verdict
    /// yet," and none of them is a sentence bae-core authored — it only hands
    /// over which of the three this candidate is in.
    var localizedText: String {
        switch self {
        case .queued: String(localized: "Waiting to be identified")
        case .running: String(localized: "Identifying\u{2026}")
        case .noAnswer:
            String(localized: "Couldn\u{2019}t complete \u{2014} will retry")
        }
    }
}
