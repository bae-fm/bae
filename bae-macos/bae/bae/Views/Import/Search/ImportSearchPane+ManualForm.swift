import BaeKit
import SwiftUI

/// The docked search form and the run it submits: the typed query's fields, the
/// providers' answers as each lands, and the lines naming the ones that did
/// not. Split from the pane itself, which owns the identify half.
extension ImportSearchPane {
    @ViewBuilder
    var manualForm: some View {
        ImportSearchFormView(
            activeTab: $activeTab,
            searchArtist: $searchArtist,
            searchAlbum: $searchAlbum,
            searchCatalog: $searchCatalog,
            searchBarcode: $searchBarcode,
            discogsEnabled: state.discogsEnabled,
            signals: state.signals,
            onSearch: onSearch,
            onOpenSettings: onOpenSettings,
        )
        if state.search != nil {
            Divider()
            searchRun
        }
    }

    /// The submitted search as its providers land: whatever has answered draws
    /// straight away, with a line for each provider still looking or failed.
    @ViewBuilder
    private var searchRun: some View {
        let groups = state.searchGroups
        if !groups.isEmpty {
            HStack {
                Spacer()
                Button("Clear", action: onClearSearch)
                    .buttonStyle(.link)
            }
            .padding(.horizontal, 18)
            ReleaseGroupListView(
                groups: groups,
                isImporting: state.isImporting,
                libraryStatuses: state.libraryStatuses,
                selectedReleaseId: state.selectedReleaseId,
                loadingReleaseId: state.loadingReleaseId,
                onSelect: onSelect,
            )
        }
        else if state.isSearching {
            ProgressView("Searching...")
                .frame(maxWidth: .infinity)
                .padding(.vertical, 32)
        }
        else if state.searchFailures.isEmpty {
            ContentUnavailableView(
                "No matches found",
                systemImage: "magnifyingglass",
                description: Text("Try different search terms"),
            )
            .frame(maxWidth: .infinity)
            .padding(.vertical, 32)
        }
        searchFailureLines
    }

    /// One line per provider that did not answer, each with its own retry.
    @ViewBuilder
    private var searchFailureLines: some View {
        ForEach(state.searchFailures, id: \.source) { source, failure in
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(
                    "\(bridgeMetadataSourceName(source: source)): "
                        + failure.badgeLine
                )
                .font(.callout)
                Button("Retry", action: onRetrySearch)
                    .buttonStyle(.link)
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 6)
        }
    }
}
