import BaeKit
import SwiftUI

/// A sheet's buttons: the secondary ones, then the one primary button, on the
/// right. With a `lead`, that button sits on the left instead, for an action
/// beside the flow rather than a step of it ("New folder…", "Use my key
/// instead", Close beside Decline and Approve).
struct MacSheetFooter<Lead: View, Trailing: View>: View {
    private let lead: Lead
    private let trailing: Trailing

    init(
        @ViewBuilder lead: () -> Lead,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.lead = lead()
        self.trailing = trailing()
    }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            lead
            Spacer()
            trailing
        }
        .padding(.top, ThemeSpace.inline)
    }
}

extension MacSheetFooter where Lead == EmptyView {
    init(@ViewBuilder _ trailing: () -> Trailing) {
        self.init(lead: { EmptyView() }, trailing: trailing)
    }
}
