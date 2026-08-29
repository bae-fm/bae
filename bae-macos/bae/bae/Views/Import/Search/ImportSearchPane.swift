import BaeKit
import SwiftUI

/// The search/identify surface, in one of two modes: what identification found
/// (the signals toolbar over its matches, or the line saying it found
/// nothing), or the typed search (the source picker, the fields, and their
/// results). Renders from `ImportSearchState` plus the form bindings and action
/// callbacks.
struct ImportSearchPane: View {
    let state: ImportSearchState
    /// Which method the stable Find online page is showing.
    @Binding
    var mode: SearchMode
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
    /// Re-identifying an existing library release can still use its local file
    /// tags directly. Folder import source changes live on the draft card.
    let onUseFileTags: (() -> Void)?
    /// Act on a signal in the toolbar — take the disc ID or barcode in or out
    /// of the run, or pick which extracted catalog number the run looks up.
    /// The state the import projection delivers is re-derived from what is
    /// left checked.
    let onToggleSignal: (BridgeSignalToggle) -> Void
    /// Start the first automatic identification run from the idle state.
    let onIdentify: () -> Void
    /// Run signal extraction and lookup again from either online-search view.
    let onRerun: () -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (BridgeMetadataResult) -> Void

    private struct FoundResult {
        let groups: [ReleaseGroup]
        let statuses: [String: BridgeLibraryStatus]
        let provenance: [String: BridgeResultProvenance]
    }

    /// The auto-identified release groups, their library statuses, and per-row
    /// provenance, extracted from the identify state.
    private var foundResult: FoundResult? {
        guard
            case .found(let groups, let statuses, _, let provenance) =
                state.identifyState
        else {
            return nil
        }
        return FoundResult(
            groups: groups,
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

    private var hasAutomaticRunStarted: Bool {
        if case .idle = state.identifyState {
            return false
        }
        return true
    }

    /// Whether the toolbar row renders: live badges when the signals are
    /// known, or just its escapes (Auto / Search manually) on a resumed
    /// verdict — a terminal state stood back up from the store, whose raw
    /// signals were never persisted and so has no badges to show.
    private var showsToolbar: Bool {
        if !state.signalsToolbar.signals.isEmpty {
            return true
        }
        switch state.identifyState {
        case .found, .notFoundAnywhere, .manualOnly:
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
                onUseFileTags: onUseFileTags,
            )
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            methodPicker
            switch mode {
            case .automatic:
                toolbar
                errorLine
                if hasAutomaticRunStarted {
                    signalsResult
                }
                else {
                    identifyAction
                }
            case .manual:
                errorLine
                manualForm
            }
        }
        .padding(.top, 6)
    }

    private var methodPicker: some View {
        Picker(selection: $mode) {
            Text("Automatic").tag(SearchMode.automatic)
            Text("Search manually").tag(SearchMode.manual)
        } label: {
            EmptyView()
        }
        .labelsHidden()
        .pickerStyle(.segmented)
        .frame(width: 300)
        .padding(.bottom, 8)
    }

    @ViewBuilder
    private var errorLine: some View {
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
    }

    // MARK: - Signals

    private var identifyAction: some View {
        Button("Identify automatically", action: onIdentify)
            .buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// What identification made of the folder: its matches, the spinner while
    /// it is still looking, or the one line saying it has nothing — with the
    /// way over to the typed search on that same line.
    @ViewBuilder
    private var signalsResult: some View {
        if let found = foundResult, !found.groups.isEmpty {
            ReleaseGroupListView(
                groups: found.groups,
                isImporting: state.isImporting,
                libraryStatuses: found.statuses,
                provenance: found.provenance,
                selectedReleaseId: state.selectedReleaseId,
                loadingReleaseId: state.loadingReleaseId,
                onSelect: onSelect,
            )
        }
        else if isTriangulating {
            // The toolbar's spinning badges carry the per-signal progress;
            // the body just notes the pipeline is still working.
            ContentUnavailableView(
                "Identifying\u{2026}",
                systemImage: "antenna.radiowaves.left.and.right",
                description: Text("Looking up the signals above."),
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        else if hasAutomaticRunStarted {
            nothingFoundLine
        }
    }

    /// The result line when nothing was found: what happened, and the one
    /// thing left to do beside it.
    @ViewBuilder
    private var nothingFoundLine: some View {
        HStack(spacing: 8) {
            Image(systemName: "info.circle.fill")
                .foregroundStyle(.orange)
            Text("No automatic matches found")
                .font(.callout)
            DiscIdInfoTip()
            Spacer()
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    // MARK: - Manual

    @ViewBuilder
    private var manualForm: some View {
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
        else if state.hasSearched && state.searchGroups.isEmpty {
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
                loadingReleaseId: state.loadingReleaseId,
                onSelect: onSelect,
            )
        }
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
            mode: SearchMode = .automatic,
            searchArtist: String = "",
            searchAlbum: String = "",
        ) -> ImportSearchPane {
            ImportSearchPane(
                state: state,
                mode: .constant(mode),
                activeTab: .constant(.general),
                activeSource: .constant(.musicBrainz),
                searchArtist: .constant(searchArtist),
                searchAlbum: .constant(searchAlbum),
                searchCatalog: .constant(""),
                searchBarcode: .constant(""),
                onSearch: {},
                onOpenSettings: {},
                onUseFileTags: nil,
                onToggleSignal: { _ in },
                onIdentify: {},
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

    #Preview("Main Pane - Auto-lookup idle") {
        ImportSearchPane.preview(state: PreviewData.searchStateIdle)
            .frame(width: 1212, height: 982)
            .importPreviewEnvironment()
    }

    #Preview("Main Pane - Manual Search") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateManual,
            mode: .manual,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title One",
        )
        .frame(width: 1212, height: 982)
        .importPreviewEnvironment()
    }

    #Preview("Main Pane - Signals named different releases") {
        ImportSearchPane.preview(state: PreviewData.searchStateDisagreement)
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
