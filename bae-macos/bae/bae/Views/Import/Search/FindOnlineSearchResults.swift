import BaeKit
import SwiftUI

/// What a typed search turned up, under its form: the album cards each
/// source has landed so far, and a line per source still looking,
/// unconfigured, or failed.
///
/// The sources answer separately, so what MusicBrainz found renders while
/// Discogs is still out — its spinner and name close the list until it lands.
struct FindOnlineSearchResults: View {
    let search: BridgeCandidateSearch
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let selectedReleaseId: String?
    let loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let onRetry: () -> Void
    let onOpenSettings: () -> Void
    let onSelect: (Pressing) -> Void

    private var groups: [ReleaseGroup] {
        search.groups.map(ReleaseGroup.init(bridge:))
    }

    var body: some View {
        // The sources' own lines close the list from inside it: a source
        // still answering belongs under what the others found, not hovering
        // under the form while the results scroll past it.
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

    /// Every source has answered and none of them knew anything. Only then:
    /// while one is still out, what it will say is not yet "nothing".
    @ViewBuilder
    private var emptyLine: some View {
        if search.status == .noMatches {
            Text("No matches \u{2014} try different terms")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
    }

    /// One line per source that has nothing to contribute yet: still looking,
    /// never asked, or failed with its way to ask again. Each carries the
    /// same glyph its cell in the ledger would.
    private var sourceLines: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(search.sourceStates, id: \.source) { source, state in
                let name = bridgeMetadataSourceName(source: source)
                switch state {
                case .searching:
                    HStack(spacing: 6) {
                        ProgressView()
                            .controlSize(.small)
                            .scaleEffect(0.6)
                            .frame(width: 11, height: 11)
                        Text(name)
                            .foregroundStyle(.tertiary)
                    }
                case .notConfigured:
                    HStack(spacing: 6) {
                        Text("\(name) not configured")
                        Button("Open Settings", action: onOpenSettings)
                            .buttonStyle(.link)
                    }
                case .failed(let failure):
                    HStack(spacing: 6) {
                        Image(systemName: "exclamationmark.triangle")
                            .foregroundStyle(.orange)
                        Text(name)
                            .foregroundStyle(.tertiary)
                            .help(failure.badgeLine)
                        Button("Retry", action: onRetry)
                            .buttonStyle(.link)
                    }
                case .done:
                    EmptyView()
                }
            }
        }
        .font(.system(size: 11))
        .foregroundStyle(.secondary)
        .padding(.leading, 28)
    }
}

extension BridgeCandidateSearch {
    /// Each source's part of the run, in the order the pane names them.
    var sourceStates:
        [(source: BridgeMetadataSource, state: BridgeSourceSearch)]
    {
        [
            (BridgeMetadataSource.musicBrainz, musicbrainz),
            (BridgeMetadataSource.discogs, discogs),
        ]
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
            onOpenSettings: {},
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
            onOpenSettings: {},
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
            onOpenSettings: {},
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
            onOpenSettings: {},
            onSelect: { _ in },
        )
        .frame(width: 660, height: 460)
        .importPreviewEnvironment()
    }
#endif
