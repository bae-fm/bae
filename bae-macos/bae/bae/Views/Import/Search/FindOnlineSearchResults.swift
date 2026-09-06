import BaeKit
import SwiftUI

/// The result area while a typed search stands: what was asked, the album
/// cards each source has landed so far, and a line per source still looking,
/// unconfigured, or failed.
///
/// The sources answer separately, so what MusicBrainz found renders while
/// Discogs is still out. Returning to identification drops the run and shows the
/// identify verdict.
struct FindOnlineSearchResults: View {
    let search: BridgeCandidateSearch
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let selectedReleaseId: String?
    let loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let onClear: () -> Void
    let onRetry: () -> Void
    let onOpenSettings: () -> Void
    let onSelect: (Pressing) -> Void

    private var groups: [ReleaseGroup] {
        search.groups.map(ReleaseGroup.init(bridge:))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            queryLine
            // The sources' own lines close the list from inside it: a source
            // still answering belongs under what the others found, not
            // hovering over the form while the results scroll past it.
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
    }

    // MARK: - What was asked

    private var queryLine: some View {
        HStack(spacing: 8) {
            FindOnlineCapsLabel("Results for")
            Text(search.query.summary)
                .font(.system(size: 11.5))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            Button(action: onClear) {
                Label(
                    "Identification results",
                    systemImage: "arrow.uturn.backward"
                )
            }
            .buttonStyle(.link)
            .font(.system(size: 12))
        }
        .padding(.horizontal, 14)
        .padding(.top, 10)
    }

    // MARK: - What came back

    /// Every source has answered and none of them knew anything. Only then:
    /// while one is still out, what it will say is not yet "nothing".
    @ViewBuilder
    private var emptyLine: some View {
        if search.noMatches {
            Text("No matches \u{2014} try different terms")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - What each source is doing

    /// One line per source that has nothing to contribute yet: still looking,
    /// never asked, or failed with its way to ask again.
    private var sourceLines: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(search.sourceStates, id: \.source) { source, state in
                let name = bridgeMetadataSourceName(source: source)
                switch state {
                case .searching:
                    HStack(spacing: 6) {
                        ProgressView()
                            .controlSize(.small)
                            .scaleEffect(0.7)
                        Text("Searching \(name)\u{2026}")
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
                        Text("\(name) search: \(failure.briefLine)")
                            .foregroundStyle(.orange)
                            .help(failure.badgeLine)
                        Button("Retry", action: onRetry)
                            .buttonStyle(.link)
                    }
                case .done:
                    EmptyView()
                }
            }
        }
        .font(.system(size: 12))
        .foregroundStyle(.secondary)
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

extension BridgeSearchQuery {
    /// What the run asked, as one line under "Results for".
    var summary: String {
        switch self {
        case .general(let artist, let album):
            [artist, album]
                .filter { !$0.isEmpty }
                .joined(separator: " \u{00b7} ")
        case .catalogNumber(let catalogNumber):
            catalogNumber
        case .barcode(let barcode):
            barcode
        }
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
            onClear: {},
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
            onClear: {},
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
            onClear: {},
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
            onClear: {},
            onRetry: {},
            onOpenSettings: {},
            onSelect: { _ in },
        )
        .frame(width: 660, height: 460)
        .importPreviewEnvironment()
    }
#endif
