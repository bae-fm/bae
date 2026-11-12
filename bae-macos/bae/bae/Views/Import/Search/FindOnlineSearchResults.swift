import BaeKit
import SwiftUI

/// A typed search's results under its form: why a barcode asked for names no
/// product, each source's album cards so far, then a line for each source
/// still searching or failed.
struct FindOnlineSearchResults: View {
    let search: BridgeCandidateSearch
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let selectedReleaseId: String?
    let loadingPick: LoadingPick?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let onRetry: () -> Void
    let onSelect: (Pressing) -> Void

    private var groups: [ReleaseGroup] {
        search.groups.map(ReleaseGroup.init(bridge:))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let unusable = search.unusableBarcode {
                Label(
                    localizedCoreString(
                        bridgeUnusableBarcodeKey(reason: unusable)
                    ),
                    systemImage: "info.circle"
                )
                .themeText(.body)
                .foregroundStyle(.secondary)
                .padding(.horizontal, ThemeSpace.group)
                .padding(.top, ThemeSpace.group)
            }
            results
        }
    }

    private var results: some View {
        // Source lines end the list, so they scroll with the results.
        ReleaseGroupListView(
            groups: groups,
            isImporting: isImporting,
            libraryStatuses: libraryStatuses,
            selectedReleaseId: selectedReleaseId,
            loadingPick: loadingPick,
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
            Text("No matches. Try different terms.")
                .themeText(.body)
                .foregroundStyle(.secondary)
        }
    }

    /// A line per source still searching, or failed with its Retry.
    private var sourceLines: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            ForEach(search.sources, id: \.source) { entry in
                let name = bridgeLookupCatalogName(catalog: entry.source)
                switch entry.state {
                case .searching:
                    HStack(spacing: ThemeSpace.compact) {
                        MacSpinner()
                            .frame(
                                width: ThemeIcon.small.size,
                                height: ThemeIcon.small.size
                            )
                        Text(
                            "Searching \(name)…",
                            comment:
                                "Find online, under a typed search's results: a catalog still searching; the argument is the catalog's name."
                        )
                        .foregroundStyle(.tertiary)
                    }
                case .failed(let failure):
                    failedLine(name, reason: failure.badgeLine)
                case .error(let failure):
                    failedLine(name, reason: failure.detail)
                // Answered sources and sources never asked add no line.
                case .done, .notConfigured, .off:
                    EmptyView()
                }
            }
        }
        .themeText(.detail)
        .foregroundStyle(.secondary)
        .padding(.leading, ThemeSpace.group)
    }

    /// A source that did not answer, with why on hover and its Retry.
    private func failedLine(_ name: String, reason: String) -> some View {
        HStack(spacing: ThemeSpace.compact) {
            Image(systemName: "exclamationmark.circle")
                .foregroundStyle(Theme.warning)
            Text(name)
                .foregroundStyle(.tertiary)
                .help(reason)
            Button("Retry", action: onRetry)
                .buttonStyle(.accentBorderless)
        }
    }
}

#if DEBUG
    // MARK: - Previews

#endif
