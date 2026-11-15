import BaeKit
import Foundation

/// One action the selection or one row's menu offers, how many rows it applies
/// to, and whether it can run now — core's answer, for the pane a selection
/// opens and the list's menu alike.
struct ImportCandidateActionOffer: Identifiable {
    let action: BridgeCandidateAction
    /// How many rows the action applies to.
    let applicable: Int
    let enabled: Bool
    /// The row a row's menu offers the action for; absent when the selection
    /// offers it.
    let rowKey: Data?
    var id: BridgeCandidateAction { action }

    /// The count shown beside the action, absent for combining: it reads every
    /// selected row as one release, so a count says nothing.
    var count: Int? { action == .combine ? nil : applicable }

    /// What the selection offers, as core's summary of it says.
    static func selection(
        _ summary: BridgeSelectionSummary
    ) -> [ImportCandidateActionOffer] {
        summary.offers.map { offer in
            ImportCandidateActionOffer(
                action: offer.action,
                applicable: Int(offer.count),
                enabled: offer.enabled,
                rowKey: nil
            )
        }
    }

    /// What the row at `key` offers on its own: each action its live state
    /// lists, in the order every surface lists them. Combining reads several
    /// rows as one release, so a lone row's menu leaves it out.
    static func row(
        _ key: Data,
        actions: [BridgeCandidateAction]
    ) -> [ImportCandidateActionOffer] {
        actions.compactMap { action in
            action == .combine
                ? nil
                : ImportCandidateActionOffer(
                    action: action,
                    applicable: 1,
                    enabled: true,
                    rowKey: key
                )
        }
    }
}
