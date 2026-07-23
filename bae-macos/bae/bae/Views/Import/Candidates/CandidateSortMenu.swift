import SwiftUI

/// The candidate-list sort menu — an ellipsis button that drops a checkmarked
/// list of sort orders and writes the chosen one back through the binding.
struct CandidateSortMenu: View {
    @Binding
    var sortOrder: CandidateSortOrder

    var body: some View {
        Menu {
            Section("Sort") {
                sortButton(.nameAZ, "Name (A\u{2013}Z)")
                sortButton(.nameZA, "Name (Z\u{2013}A)")
                sortButton(.dateAddedNewest, "Date Added (Newest)")
                sortButton(.dateAddedOldest, "Date Added (Oldest)")
            }
        } label: {
            Image(systemName: "ellipsis.circle")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
    }

    @ViewBuilder
    private func sortButton(
        _ order: CandidateSortOrder,
        _ label: LocalizedStringKey
    ) -> some View {
        Button {
            sortOrder = order
        } label: {
            if sortOrder == order {
                Label(label, systemImage: "checkmark")
            }
            else {
                Text(label)
            }
        }
    }
}

#if DEBUG
    #Preview("Candidate sort menu") {
        @Previewable
        @State
        var order: CandidateSortOrder = .dateAddedNewest
        CandidateSortMenu(sortOrder: $order)
            .padding()
            .windowBackground()
    }
#endif
