import BaeKit
import SwiftUI

/// The side pane docked at the window's right. Its top holds the tabs, centred,
/// and Close at its end, with `lead` at its start; the tab's content fills the
/// rest. `MacQueuePanelContainer` reads the stores for each tab.
struct MacQueuePanel<Content: View>: View {
    let tab: SidePaneTab
    let onSelectTab: (SidePaneTab) -> Void
    var lead: SidePaneLead = .none
    let onClose: () -> Void
    var width: CGFloat = 420
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            MacSidePaneHeader(
                tab: tab,
                onSelectTab: onSelectTab,
                lead: lead,
                onClose: onClose
            )
            content()
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .top
                )
        }
        .frame(width: width)
        .frame(maxHeight: .infinity)
        // Sidebar chrome: one surface step above the content beside it, and
        // a hairline on the docked edge.
        .background(Theme.surface)
        .overlay(alignment: .leading) {
            MacHairline(axis: .vertical)
        }
    }
}
