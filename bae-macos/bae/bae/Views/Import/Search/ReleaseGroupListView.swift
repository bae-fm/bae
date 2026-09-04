import BaeKit
import SwiftUI

/// Scrollable list of release-group cards, each with its pressing rows hanging
/// beneath on a connecting rule. Picking a pressing reports it via `onSelect`;
/// the surrounding flow opens the docked confirm pane. The currently-docked
/// pressing (if any) renders selected.
///
/// `trailing` closes the list from inside the scroll, so what the list has to
/// say about itself — a source whose results are missing, a source still
/// answering — scrolls with the results rather than hovering under them.
struct ReleaseGroupListView<Trailing: View>: View {
    let groups: [ReleaseGroup]
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    /// Per-release provenance keyed by release id, for the signal badges.
    /// Empty for typed-search results (no identify signals produced them).
    var provenance: [String: BridgeResultProvenance] = [:]
    /// Release id of the pressing whose confirm pane is open, if any.
    let selectedReleaseId: String?
    /// Release id whose candidate detail is being fetched, if any.
    let loadingReleaseId: String?
    let onSelect: (Pressing) -> Void
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 14) {
                ForEach(groups) { group in
                    ReleaseGroupSection(
                        group: group,
                        isImporting: isImporting,
                        libraryStatuses: libraryStatuses,
                        provenance: provenance,
                        selectedReleaseId: selectedReleaseId,
                        loadingReleaseId: loadingReleaseId,
                        onSelect: onSelect,
                    )
                }
                trailing()
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// A line closing a result list: what is not in it, and why. Sits where the
/// next group would be, in the same indent as the pressing rows.
struct MissingSourceNote: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 11))
            .foregroundStyle(.tertiary)
            .padding(.leading, 28)
    }
}

/// One release group: its card with the pressing rows hanging beneath on a
/// connecting rule.
struct ReleaseGroupSection: View {
    let group: ReleaseGroup
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    var provenance: [String: BridgeResultProvenance] = [:]
    let selectedReleaseId: String?
    /// The pressing whose pick is being read right now — its row carries a
    /// spinner while the list stays put.
    var loadingReleaseId: String?
    let onSelect: (Pressing) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ReleaseGroupCard(group: group)
            pressings
        }
    }

    /// The group's pressing rows, indented under a hairline rule that ties them
    /// to the card above.
    private var pressings: some View {
        HStack(spacing: 0) {
            Rectangle()
                .fill(.white.opacity(0.07))
                .frame(width: 1)
            VStack(spacing: 1) {
                ForEach(group.pressings) { pressing in
                    ImportSearchResultRow(
                        pressing: pressing,
                        isImporting: isImporting,
                        libraryStatus: libraryStatuses[pressing.id],
                        provenance: provenance[pressing.id],
                        isSelected: isSelected(pressing),
                        isLoading: isLoading(pressing),
                        onSelect: onSelect,
                    )
                }
            }
            .padding(.leading, 6)
        }
        .padding(.leading, 16)
    }

    /// A pressing is the docked one when any of its sources' releases is: a
    /// person who took the Discogs half of a merged row still picked this row.
    private func isSelected(_ pressing: Pressing) -> Bool {
        pressing.releases.contains {
            $0.releaseId == selectedReleaseId
                || $0.releaseId == loadingReleaseId
        }
    }

    private func isLoading(_ pressing: Pressing) -> Bool {
        pressing.releases.contains { $0.releaseId == loadingReleaseId }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Exact match") {
        ReleaseGroupListView(
            groups: [PreviewData.searchGroupExact],
            isImporting: false,
            libraryStatuses: [:],
            provenance: PreviewData.searchProvenanceExact,
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onSelect: { _ in },
            trailing: { EmptyView() },
        )
        .frame(width: 620, height: 520)
        .importPreviewEnvironment()
    }

    #Preview("Manual results") {
        ReleaseGroupListView(
            groups: PreviewData.searchGroupsManual,
            isImporting: false,
            libraryStatuses: [:],
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onSelect: { _ in },
            trailing: { EmptyView() },
        )
        .frame(width: 620, height: 520)
        .importPreviewEnvironment()
    }

    #Preview("A source's results are missing") {
        ReleaseGroupListView(
            groups: [PreviewData.searchGroupExact],
            isImporting: false,
            libraryStatuses: [:],
            provenance: PreviewData.searchProvenanceExact,
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onSelect: { _ in },
            trailing: {
                MissingSourceNote(
                    text: String(
                        localized:
                            "\(bridgeMetadataSourceName(source: .discogs)) \(SignalBadgeStyle.sentenceLabel(for: .barcode)) results are missing from this list."
                    )
                )
            },
        )
        .frame(width: 620, height: 520)
        .importPreviewEnvironment()
    }
#endif
