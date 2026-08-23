import BaeKit
import SwiftUI

/// The composer/artist browser's master list: virtualized rows over a
/// `PaginatedList`, each visible row loading the page it sits in.
struct BrowseList<Row: Identifiable & Sendable, RowView: View>: View
where Row.ID: Sendable {
    let list: PaginatedList<Row>
    let row: (Int) -> RowView

    var body: some View {
        List {
            ForEach(0..<list.totalCount, id: \.self) { index in
                row(index)
                    .listRowInsets(
                        EdgeInsets(top: 0, leading: 10, bottom: 2, trailing: 10)
                    )
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
                    .task(id: RowLoadID(epoch: list.loadEpoch, index: index)) {
                        await list.loadPage(containing: index)
                    }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .contentMargins(.top, 4, for: .scrollContent)
        .contentMargins(.bottom, 20, for: .scrollContent)
        .reportsHeaderScroll(id: "browseList")
        .background(Theme.background)
    }
}

#if DEBUG
    #Preview("Browse List") {
        let libraryStore = PreviewData.seededComposerStore()
        let list = PreviewData.composerList()
        return BrowseList(list: list) { index in
            let id = list.idAt(index)
            return BrowseListRow(
                id: id,
                isSelected: index == 0,
                summaries: \.composerSummaries,
                select: { _ in }
            )
        }
        .frame(width: 340, height: 600)
        .environment(libraryStore)
        .environment(ImageStore.stub())
    }
#endif
