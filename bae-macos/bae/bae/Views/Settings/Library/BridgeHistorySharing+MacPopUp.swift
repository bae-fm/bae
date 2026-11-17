import BaeKit
import Foundation

extension BridgeHistorySharing {
    /// Who the Listening history pop-up offers to share the person's plays
    /// with.
    @MacPopUpBuilder<BridgeHistorySharing>
    static var popUpOptions: [MacPopUpEntry<BridgeHistorySharing>] {
        MacPopUpOption("Off", value: BridgeHistorySharing.off)
        MacPopUpOption("Only me", value: BridgeHistorySharing.private)
        MacPopUpOption("Everyone", value: BridgeHistorySharing.household)
    }
}
