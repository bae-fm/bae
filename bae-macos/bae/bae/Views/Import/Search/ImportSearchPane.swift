import BaeKit
import SwiftUI

/// The search/identify surface: the signals toolbar, an identify-state banner,
/// and either the auto-identified matches, the manual-search form + its
/// results, or the per-signal conflict surface. Renders from `ImportSearchState`
/// plus the form bindings and action callbacks.
struct ImportSearchPane: View {
    let state: ImportSearchState
    @Binding
    var activeTab: SearchTab
    @Binding
    var activeSource: BridgeMetadataSource
    @Binding
    var searchArtist: String
    @Binding
    var searchAlbum: String
    @Binding
    var searchCatalog: String
    @Binding
    var searchBarcode: String
    let onSearch: () -> Void
    let onOpenSettings: () -> Void
    let onSearchManually: () -> Void
    /// Return from the manual-search form to the auto-identified matches.
    let onViewMatches: () -> Void
    /// Set when the candidate can seed from local files (folder imports always
    /// can). `nil` suppresses the "Add as Unknown" link.
    let onAddAsUnknown: (() -> Void)?
    /// Toggle a signal in the toolbar — include / exclude it from
    /// triangulation. Drops the signal from the in-memory combine step (no
    /// re-fetch) and re-derives the state delivered by the import projection.
    /// The conflict surface's per-signal "Ignore" links route through here too.
    let onToggleSignal: (BridgeExcludedSignal) -> Void
    /// Re-run the lookups from the toolbar's `Re-run` action.
    let onRerun: () -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (BridgeMetadataResult) -> Void

    private struct FoundResult {
        let group: ReleaseGroup
        let statuses: [String: BridgeLibraryStatus]
        let provenance: [String: BridgeResultProvenance]
    }

    /// The auto-identified release group, its library statuses, and per-row
    /// provenance, extracted from the identify state. A `Found` state always
    /// carries exactly one group.
    private var foundResult: FoundResult? {
        guard
            case .found(let group, let statuses, _, let provenance) =
                state.identifyState
        else {
            return nil
        }
        return FoundResult(
            group: group,
            statuses: statuses,
            provenance: provenance
        )
    }

    /// Whether the pipeline is mid-triangulation — drives the body's
    /// "Identifying…" placeholder (the per-signal progress lives in the
    /// toolbar's spinning badges now).
    private var isTriangulating: Bool {
        if case .triangulating = state.identifyState {
            return true
        }
        return false
    }

    /// Whether the toolbar row renders: live badges when the signals are
    /// known, or just its escapes (Re-run / Search manually) on a resumed
    /// verdict — a terminal state stood back up from the store, whose raw
    /// signals were never persisted and so has no badges to show.
    private var showsToolbar: Bool {
        if !state.signalsToolbar.signals.isEmpty {
            return true
        }
        switch state.identifyState {
        case .found, .conflict, .notFoundAnywhere, .manualOnly:
            return true
        case .idle, .triangulating:
            return false
        }
    }

    /// The signals toolbar, shown across every state once core has emitted a
    /// transition. Hidden until then (empty badge list) and in idle.
    @ViewBuilder
    private var toolbar: some View {
        if showsToolbar {
            SignalsToolbarView(
                toolbar: state.signalsToolbar,
                onToggle: onToggleSignal,
                onRerun: onRerun,
                onSearchManually: onSearchManually,
                onAddAsUnknown: state.showManualSearch ? nil : onAddAsUnknown,
            )
        }
    }

    var body: some View {
        // Conflict replaces the standard banner + results layout entirely:
        // it stacks per-signal sections so the user can pick a row or
        // toggle a signal. `showManualSearch` skips it — the user
        // explicitly asked for the manual form. `notFoundAnywhere` keeps
        // the flat banner for the truly-empty case.
        if case .conflict = state.identifyState, !state.showManualSearch {
            ImportConflictView(
                state: state,
                onToggle: onToggleSignal,
                onRerun: onRerun,
                onSearchManually: onSearchManually,
                onAddAsUnknown: onAddAsUnknown,
                onSelect: onSelect,
            )
        }
        else {
            normalBody
        }
    }

    @ViewBuilder
    private var normalBody: some View {
        let found = foundResult
        let showingAutoMatches = found != nil && !state.showManualSearch

        VStack(spacing: 0) {
            toolbar

            identifyBanner

            if let error = state.error {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(error)
                }
                .font(.caption)
                .foregroundStyle(.red)
                .padding(.horizontal)
                .padding(.vertical, 6)
            }

            if showingAutoMatches, let found {
                ReleaseGroupListView(
                    groups: [found.group],
                    isImporting: state.isImporting,
                    libraryStatuses: found.statuses,
                    provenance: found.provenance,
                    selectedReleaseId: state.selectedReleaseId,
                    onSelect: onSelect,
                )
            }
            else if isTriangulating, !state.showManualSearch {
                // The toolbar's spinning badges carry the per-signal progress;
                // the body just notes the pipeline is still working.
                ContentUnavailableView(
                    "Identifying\u{2026}",
                    systemImage: "antenna.radiowaves.left.and.right",
                    description: Text("Looking up the signals above."),
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            else {
                // In manual search with an auto-identified match available,
                // offer the way back to it — the toolbar's "Search manually"
                // has no reciprocal otherwise.
                if let found {
                    Button {
                        onViewMatches()
                    } label: {
                        Label(
                            "View automatic matches (\(found.group.pressings.count))",
                            systemImage: "chevron.left",
                        )
                    }
                    .buttonStyle(.link)
                    .font(.callout)
                    .padding(.horizontal, 18)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                ImportSearchFormView(
                    activeTab: $activeTab,
                    activeSource: $activeSource,
                    searchArtist: $searchArtist,
                    searchAlbum: $searchAlbum,
                    searchCatalog: $searchCatalog,
                    searchBarcode: $searchBarcode,
                    discogsEnabled: state.discogsEnabled,
                    signals: state.signals,
                    onSearch: onSearch,
                    onOpenSettings: onOpenSettings,
                )
                Divider()

                if state.isSearching {
                    ProgressView("Searching...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                else if !state.hasSearched {
                    ContentUnavailableView(
                        "No results",
                        systemImage: "magnifyingglass",
                        description: Text(
                            "Search MusicBrainz or Discogs to find metadata"
                        ),
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                else if state.searchGroups.isEmpty {
                    ContentUnavailableView(
                        "No matches found",
                        systemImage: "magnifyingglass",
                        description: Text(
                            "Try different search terms or another source"
                        ),
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                else {
                    ReleaseGroupListView(
                        groups: state.searchGroups,
                        isImporting: state.isImporting,
                        libraryStatuses: state.libraryStatuses,
                        selectedReleaseId: state.selectedReleaseId,
                        onSelect: onSelect,
                    )
                }
            }
        }
        .padding(.top, 6)
    }

    @ViewBuilder
    private var identifyBanner: some View {
        // The toolbar above owns the global escapes (Search manually / Skip
        // identifying / Re-run), so the banner carries only the status copy for
        // the states that need it. Error has no toolbar, so it keeps its own
        // escape.
        switch state.identifyState {
        case .triangulating, .found, .idle:
            // Triangulation is covered by the toolbar's spinning badges and the
            // body placeholder; a found match speaks for itself through the
            // toolbar signals and the release-group card below. Neither needs a
            // banner.
            EmptyView()
        case .notFoundAnywhere:
            HStack(spacing: 8) {
                Image(systemName: "info.circle.fill")
                    .foregroundStyle(.orange)
                Text("No automatic matches found")
                    .font(.callout)
                discIdInfoIcon
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .overlay(alignment: .bottom) {
                Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
            }
        case .manualOnly:
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass.circle.fill")
                    .foregroundStyle(.secondary)
                Text("No identifying signals yet — search manually")
                    .font(.callout)
                discIdInfoIcon
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .overlay(alignment: .bottom) {
                Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
            }
        case .conflict:
            // The full per-signal surface renders from `body` when not in
            // manual-search mode. This banner is the manual-search-mode
            // status line; the toolbar carries the escapes.
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.octagon.fill")
                    .foregroundStyle(Theme.accent)
                Text("Signals disagree on identity")
                    .font(.callout)
                discIdInfoIcon
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .overlay(alignment: .bottom) {
                Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
            }
        }
    }

    private var discIdInfoIcon: some View {
        DiscIdInfoTip()
    }
}

#if DEBUG
    // MARK: - Previews

    extension ImportSearchPane {
        /// Preview builder — fixes the form bindings and action callbacks to inert
        /// defaults so a preview states only the result situation it exercises.
        @MainActor
        static func preview(
            state: ImportSearchState,
            searchArtist: String = "",
            searchAlbum: String = "",
        ) -> ImportSearchPane {
            ImportSearchPane(
                state: state,
                activeTab: .constant(.general),
                activeSource: .constant(.musicBrainz),
                searchArtist: .constant(searchArtist),
                searchAlbum: .constant(searchAlbum),
                searchCatalog: .constant(""),
                searchBarcode: .constant(""),
                onSearch: {},
                onOpenSettings: {},
                onSearchManually: {},
                onViewMatches: {},
                onAddAsUnknown: {},
                onToggleSignal: { _ in },
                onRerun: {},
                onSelect: { _ in },
            )
        }
    }

    #Preview("Main Pane - Exact Matches") {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
            .frame(width: 1212, height: 982)
            .importPreviewEnvironment()
    }

    #Preview("Main Pane - Manual Search") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateManual,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title One",
        )
        .frame(width: 1212, height: 982)
        .importPreviewEnvironment()
    }

    #Preview("Main Pane - Conflict") {
        ImportSearchPane.preview(state: PreviewData.searchStateConflict)
            .frame(width: 1212, height: 982)
            .importPreviewEnvironment()
    }

    #Preview("Main Pane - Auto-lookup in progress") {
        ImportSearchPane.preview(state: PreviewData.searchStateTriangulating)
            .frame(width: 1212, height: 982)
            .importPreviewEnvironment()
    }

    #Preview("Main Pane - Nothing found") {
        ImportSearchPane.preview(state: PreviewData.searchStateNotFound)
            .frame(width: 1212, height: 982)
            .importPreviewEnvironment()
    }
#endif
