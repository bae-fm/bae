import BaeKit
import SwiftUI

/// The composer/artist browser's master list: virtualized rows over a
/// `PaginatedList`; each row on screen keeps the list loading that row until
/// the row leaves the screen.
struct BrowseList<Row: Identifiable & Sendable & Equatable, RowView: View>: View
where Row.ID: Sendable {
    let list: PaginatedList<Row>
    let row: (Int) -> RowView
    /// The list's height on screen.
    @State
    private var height: CGFloat = 0
    /// The height of the shortest row the list has drawn, placeholders
    /// included.
    @State
    private var rowHeight: CGFloat?

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
                    .onGeometryChange(for: CGFloat.self) {
                        $0.size.height
                    } action: {
                        drawn in
                        guard drawn > 0 else { return }
                        rowHeight = min(rowHeight ?? drawn, drawn)
                    }
            }
        }
        .onGeometryChange(for: CGFloat.self) {
            $0.size.height
        } action: {
            height = $0
        }
        // The list sizes its window from the rows the screen fits.
        .onChange(of: screenRows, initial: true) { _, rows in
            list.screenRows = rows
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

    /// How many rows the list fits: its height over its shortest row, and
    /// one more for the rows the edges cut.
    private var screenRows: Int? {
        guard let rowHeight, height > 0 else { return nil }
        return Int(ceil(height / rowHeight)) + 1
    }
}
