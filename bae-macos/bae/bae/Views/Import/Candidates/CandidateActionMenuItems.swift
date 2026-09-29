import BaeKit
import SwiftUI

/// What a row's menu acts on: that row alone, or the selection it is part of.
struct CandidateActionMenu {
    let offers: [ImportCandidateActionOffer]
    /// Whether the menu is a selection's rather than one row's.
    let isSelection: Bool

    static let empty = CandidateActionMenu(offers: [], isSelection: false)
}

/// What a row's menu offers, grouped as the pane a selection opens groups it:
/// the same offers, in the same order, so nothing is on one and missing from
/// the other. One row's menu names its actions for that row; a selection's
/// names each with how many folders it applies to.
///
/// `menu` is asked for in this view's body, which a context menu evaluates
/// when it opens rather than when the row it hangs off is drawn.
struct CandidateActionMenuItems: View {
    let menu: () -> CandidateActionMenu
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        let menu = menu()
        let offers = menu.offers
        let groups = ImportBulkActionGroup.allCases.filter { group in
            offers.contains { group.actions.contains($0.action) }
        }
        ForEach(groups) { group in
            if group != groups.first {
                Divider()
            }
            ForEach(offers.filter { group.actions.contains($0.action) }) {
                offer in
                Button(title(offer, in: menu)) { onPerform(offer) }
                    .disabled(!offer.enabled)
            }
        }
    }

    private func title(
        _ offer: ImportCandidateActionOffer,
        in menu: CandidateActionMenu
    ) -> String {
        guard menu.isSelection else { return offer.action.rowLabel }
        guard let count = offer.count else { return offer.action.label }
        return offer.action.label(count: count)
    }
}
