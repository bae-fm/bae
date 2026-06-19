import SwiftUI

/// Scrollable list of release-group cards, each with its pressing rows hanging
/// beneath on a connecting rule. Clicking a pressing row reports the pick via
/// `onSelect`; the surrounding flow opens the docked confirm pane. The
/// currently-docked pressing (if any) renders selected.
struct ReleaseGroupListView: View {
    let groups: [ReleaseGroup]
    let isImporting: Bool
    let libraryStatuses: [String: LibraryStatus]
    /// Per-release provenance keyed by release id, for the signal badges.
    /// Empty for manual-search results (no auto-identify signals).
    var provenance: [String: ResultProvenance] = [:]
    /// Release id of the pressing whose confirm pane is open, if any.
    let selectedReleaseId: String?
    let onSelect: (MetadataResult) -> Void

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 18) {
                ForEach(groups) { group in
                    VStack(alignment: .leading, spacing: 0) {
                        ReleaseGroupCard(group: group)
                        pressings(of: group)
                    }
                }
            }
            .padding(14)
        }
    }

    /// The group's pressing rows, indented under a hairline rule that ties them
    /// to the card above.
    private func pressings(of group: ReleaseGroup) -> some View {
        HStack(spacing: 0) {
            Rectangle()
                .fill(.white.opacity(0.07))
                .frame(width: 1)
            VStack(spacing: 2) {
                ForEach(group.pressings) { pressing in
                    ImportSearchResultRow(
                        result: pressing,
                        isImporting: isImporting,
                        libraryStatus: libraryStatuses[pressing.releaseId],
                        provenance: provenance[pressing.releaseId],
                        isSelected: pressing.releaseId == selectedReleaseId,
                        onSelect: { onSelect(pressing) },
                    )
                }
            }
            .padding(.leading, 8)
        }
        .padding(.leading, 12)
        .padding(.top, 4)
    }
}
