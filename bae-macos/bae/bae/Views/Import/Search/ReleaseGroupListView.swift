import BaeKit
import SwiftUI

/// A scrolling list of release-group cards with their pressing rows, closed
/// by `trailing` inside the scroll so it scrolls with the results.
struct ReleaseGroupListView<Trailing: View>: View {
    let groups: [ReleaseGroup]
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    /// What the candidate's own text agrees with about each release, keyed by
    /// release id; empty for typed-search results.
    var agreements: [String: BridgeAgreements] = [:]
    /// Release id of the pressing whose confirm pane is open, if any.
    let selectedReleaseId: String?
    /// Release id whose candidate detail is being fetched, if any.
    let loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let onSelect: (Pressing) -> Void
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        ScrollView {
            ReleaseGroupListContent(
                groups: groups,
                isImporting: isImporting,
                libraryStatuses: libraryStatuses,
                agreements: agreements,
                selectedReleaseId: selectedReleaseId,
                loadingReleaseId: loadingReleaseId,
                releaseSelectionFailure: releaseSelectionFailure,
                onSelect: onSelect,
                trailing: trailing,
            )
        }
    }
}

/// The list itself, for a scroll that holds more than the list — the ledger
/// above it in the AUTOMATIC section.
struct ReleaseGroupListContent<Trailing: View>: View {
    let groups: [ReleaseGroup]
    /// Whether the rows a run's agreement set aside show on their cards.
    var showsNarrowedOut = false
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    var agreements: [String: BridgeAgreements] = [:]
    let selectedReleaseId: String?
    let loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    /// Identify the candidate again, reading once more the documents a run
    /// could not; `nil` where no run read any, as for a typed search.
    var onRetryUnread: (() -> Void)?
    let onSelect: (Pressing) -> Void
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        LazyVStack(alignment: .leading, spacing: ThemeSpace.group) {
            ForEach(groups) { group in
                ReleaseGroupSection(
                    group: group,
                    showsNarrowedOut: showsNarrowedOut,
                    isImporting: isImporting,
                    libraryStatuses: libraryStatuses,
                    agreements: agreements,
                    selectedReleaseId: selectedReleaseId,
                    loadingReleaseId: loadingReleaseId,
                    releaseSelectionFailure: releaseSelectionFailure,
                    onRetryUnread: onRetryUnread,
                    onSelect: onSelect,
                )
            }
            trailing()
        }
        .padding(ThemeSpace.group)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The album a section of a card's rows are pressings of: its own title, and
/// its catalog's name opening its page there.
struct AlbumSectionHeading: View {
    let album: BridgeAlbumHeading

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Text(album.title)
                .themeText(.strong)
                .lineLimit(1)
                .truncationMode(.tail)
            AlbumSourceLink(source: album.source)
        }
        .padding(.top, ThemeSpace.related)
        .padding(.bottom, ThemeSpace.line)
        .padding(.leading, ThemeSpace.related)
    }
}

/// A line closing a result list: what is not in it, and why. Sits where the
/// next group would be, in the same indent as the pressing rows.
struct MissingSourceNote: View {
    let text: String

    var body: some View {
        Text(text)
            .themeText(.detail)
            .foregroundStyle(.tertiary)
            .padding(.leading, ReleaseGroupSection.rowTextInset)
    }
}

/// One release group: its card with the pressing rows hanging beneath on a
/// connecting rule.
struct ReleaseGroupSection: View {
    /// How far the rule sits in from the card's leading edge.
    private static let ruleInset = ThemeSpace.edge
    private static let ruleWidth: CGFloat = 1
    /// The gap between the rule and the rows.
    private static let ruleGap = ThemeSpace.compact
    /// Where a pressing row's text starts, from the card's leading edge.
    static let rowTextInset =
        ruleInset + ruleWidth + ruleGap
        + ImportSearchResultRow.horizontalPadding

    let group: ReleaseGroup
    /// Whether the rows a run's agreement set aside show beneath the offered
    /// ones.
    var showsNarrowedOut = false
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    var agreements: [String: BridgeAgreements] = [:]
    let selectedReleaseId: String?
    /// The pressing whose pick is being read right now.
    var loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    /// Identify the candidate again, reading once more the documents a run
    /// could not; `nil` where no run read any, as for a typed search.
    var onRetryUnread: (() -> Void)?
    let onSelect: (Pressing) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ReleaseGroupCard(group: group)
            pressings
        }
    }

    /// The group's pressing rows on a rule under the card, album by album,
    /// with the rows set aside after the offered ones.
    private var pressings: some View {
        HStack(spacing: 0) {
            Rectangle()
                .fill(Theme.hairline)
                .frame(width: Self.ruleWidth)
            VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                ForEach(Array(group.sections.enumerated()), id: \.offset) {
                    _,
                    section in
                    let rows =
                        showsNarrowedOut
                        ? section.pressings + section.narrowedOut
                        : section.pressings
                    if let album = section.album, !rows.isEmpty {
                        AlbumSectionHeading(album: album)
                    }
                    ForEach(rows) { pressing in
                        ImportSearchResultRow(
                            pressing: pressing,
                            isImporting: isImporting,
                            libraryStatus: libraryStatuses[pressing.id],
                            agreements: agreements[pressing.id],
                            isSelected: isSelected(pressing),
                            isLoading: isLoading(pressing),
                            failure: releaseSelectionFailure,
                            onRetryUnread: onRetryUnread,
                            onSelect: onSelect,
                        )
                    }
                }
            }
            .padding(.leading, Self.ruleGap)
        }
        .padding(.leading, Self.ruleInset)
    }

    /// Whether any of the pressing's releases is the one the draft carries.
    func isSelected(_ pressing: Pressing) -> Bool {
        pressing.releases.contains { $0.releaseId == selectedReleaseId }
    }

    /// Whether this pressing is the one whose pick is being read right now.
    func isLoading(_ pressing: Pressing) -> Bool {
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
            agreements: PreviewData.searchAgreementsExact,
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
            agreements: PreviewData.searchAgreementsExact,
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onSelect: { _ in },
            trailing: {
                MissingSourceNote(
                    text: String(
                        localized:
                            "\(bridgeCatalogName(catalog: .discogs)) \(SignalBadgeStyle.sentenceLabel(for: .barcode)) results are missing from this list."
                    )
                )
            },
        )
        .frame(width: 620, height: 520)
        .importPreviewEnvironment()
    }
#endif
