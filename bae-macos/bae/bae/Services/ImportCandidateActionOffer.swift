import BaeKit
import Foundation

/// One action the selection or one row's menu offers, the number shown
/// beside it, and whether it can run now — core's answer, for the pane a
/// selection opens and the list's menu alike.
struct ImportCandidateActionOffer: Identifiable {
    let action: BridgeCandidateAction
    /// The number shown beside the action: how many rows it runs on.
    let applicable: Int
    let enabled: Bool
    /// The row a row's menu offers the action for; absent when the selection
    /// offers it.
    let rowKey: Data?
    var id: BridgeCandidateAction { action }

    /// One of core's offers, for the row at `rowKey` or, with none, for the
    /// selection.
    init(_ offer: BridgeSelectionOffer, rowKey: Data?) {
        action = offer.action
        applicable = Int(offer.count)
        enabled = offer.enabled
        self.rowKey = rowKey
    }

    /// What the selection offers, as core's summary of it says.
    static func selection(
        _ summary: BridgeSelectionSummary
    ) -> [ImportCandidateActionOffer] {
        summary.offers.map { ImportCandidateActionOffer($0, rowKey: nil) }
    }
}
