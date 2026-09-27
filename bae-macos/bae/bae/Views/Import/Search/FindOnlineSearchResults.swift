import BaeKit
import SwiftUI

/// A typed search's results under its form: each source's album cards so far,
/// then a line for each source still searching or failed.
struct FindOnlineSearchResults: View {
    let search: BridgeCandidateSearch
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let selectedReleaseId: String?
    let loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let onRetry: () -> Void
    let onSelect: (Pressing) -> Void

    private var groups: [ReleaseGroup] {
        search.groups.map(ReleaseGroup.init(bridge:))
    }

    var body: some View {
        // Source lines end the list, so they scroll with the results.
        ReleaseGroupListView(
            groups: groups,
            isImporting: isImporting,
            libraryStatuses: libraryStatuses,
            selectedReleaseId: selectedReleaseId,
            loadingReleaseId: loadingReleaseId,
            releaseSelectionFailure: releaseSelectionFailure,
            onSelect: onSelect,
            trailing: {
                emptyLine
                sourceLines
            },
        )
    }

    /// Shown only once every source has answered with nothing.
    @ViewBuilder
    private var emptyLine: some View {
        if search.status == .noMatches {
            Text("No matches \u{2014} try different terms")
                .themeText(.body)
                .foregroundStyle(.secondary)
        }
    }

    /// A line per source still searching, or failed with its Retry.
    private var sourceLines: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            ForEach(search.sources, id: \.source) { entry in
                let name = bridgeCatalogName(catalog: entry.source)
                switch entry.state {
                case .searching:
                    HStack(spacing: ThemeSpace.compact) {
                        ProgressView()
                            .controlSize(.small)
                            .scaleEffect(0.6)
                            .frame(
                                width: ThemeIcon.small.size,
                                height: ThemeIcon.small.size
                            )
                        Text(name)
                            .foregroundStyle(.tertiary)
                    }
                case .failed(let failure):
                    HStack(spacing: ThemeSpace.compact) {
                        Image(systemName: "exclamationmark.triangle")
                            .foregroundStyle(Theme.warning)
                        Text(name)
                            .foregroundStyle(.tertiary)
                            .help(failure.badgeLine)
                        Button("Retry", action: onRetry)
                            .buttonStyle(.link)
                    }
                // Answered sources and sources never asked add no line.
                case .done, .notConfigured, .off:
                    EmptyView()
                }
            }
        }
        .themeText(.detail)
        .foregroundStyle(.secondary)
        .padding(.leading, ReleaseGroupSection.rowTextInset)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Search results") {
        FindOnlineSearchResults(
            search: PreviewData.manualSearchRun,
            isImporting: false,
            libraryStatuses: [:],
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onRetry: {},
            onSelect: { _ in },
        )
        .frame(width: 660, height: 460)
        .importPreviewEnvironment()
    }

    #Preview("Searching") {
        FindOnlineSearchResults(
            search: PreviewData.searchRunInFlight,
            isImporting: false,
            libraryStatuses: [:],
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onRetry: {},
            onSelect: { _ in },
        )
        .frame(width: 660, height: 460)
        .importPreviewEnvironment()
    }

    #Preview("A source failed") {
        FindOnlineSearchResults(
            search: PreviewData.searchRunSourceFailed,
            isImporting: false,
            libraryStatuses: [:],
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onRetry: {},
            onSelect: { _ in },
        )
        .frame(width: 660, height: 460)
        .importPreviewEnvironment()
    }

    #Preview("Nothing matched") {
        FindOnlineSearchResults(
            search: PreviewData.searchRunEmpty,
            isImporting: false,
            libraryStatuses: [:],
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onRetry: {},
            onSelect: { _ in },
        )
        .frame(width: 660, height: 460)
        .importPreviewEnvironment()
    }
#endif
