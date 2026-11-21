import BaeKit
import SwiftUI

/// A typed search's results under its form: why a barcode asked for names no
/// product, each source's album cards so far, then a line for each source
/// still searching or failed.
struct FindOnlineSearchResults: View {
    let search: BridgeCandidateSearch
    let isImporting: Bool
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
                Label {
                    Text(
                        verbatim: localizedCoreString(
                            bridgeUnusableBarcodeKey(reason: unusable)
                        )
                    )
                } icon: {
                    Image(systemName: "info.circle")
                        .accessibilityHidden(true)
                }
                .themeText(.body)
                .foregroundStyle(Theme.macTextSecondary)
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
                .foregroundStyle(Theme.macTextSecondary)
        }
    }

    /// A line per source still searching, or failed with its Retry.
    private var sourceLines: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            ForEach(search.sources, id: \.source) { entry in
                let name = bridgeLookupCatalogName(catalog: entry.source)
                switch entry.state {
                case .searching:
                    MacSearchSourceLine(
                        catalog: name,
                        status: .searching,
                        onRetry: onRetry
                    )
                case .failed(let failure):
                    MacSearchSourceLine(
                        catalog: name,
                        status: .failed(reason: failure.badgeLine),
                        onRetry: onRetry
                    )
                case .error(let failure):
                    MacSearchSourceLine(
                        catalog: name,
                        status: .failed(reason: failure.detail),
                        onRetry: onRetry
                    )
                // Answered sources and sources never asked add no line.
                case .done, .notConfigured, .off:
                    EmptyView()
                }
            }
        }
    }
}
