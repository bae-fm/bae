import BaeKit
import SwiftUI

/// The composer/artist browser's master list: virtualized rows over a
/// `PaginatedList`; each row on screen keeps the list loading that row until
/// the row leaves the screen.
struct BrowseList<Row: Identifiable & Sendable & Equatable, RowView: View>: View
where Row.ID: Sendable {
    let list: PaginatedList<Row>
    let row: (Int) -> RowView

    var body: some View {
        List {
            ForEach(0..<list.positionCount, id: \.self) { index in
                row(index)
                    .plainListRowEdge(
                        ThemeSpace.related,
                        bottom: ThemeSpace.hairline
                    )
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
                    .task(id: RowLoadID(epoch: list.loadEpoch, index: index)) {
                        await list.holdPage(containing: index)
                    }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        // Not `contentMargins`: a plain macOS `List` ignores it. Safe-area
        // padding insets the rows the way the canvas's list padding does.
        .safeAreaPadding(.top, ThemeSpace.inline)
        .safeAreaPadding(.bottom, ThemeSpace.section)
        .reportsHeaderScroll(id: "browseList")
        .background(Theme.background)
    }
}
