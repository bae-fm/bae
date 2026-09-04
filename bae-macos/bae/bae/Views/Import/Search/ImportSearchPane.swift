import BaeKit
import SwiftUI

/// Find online: one page. The header states what identification concluded and
/// offers the one thing to do about it, the result area lists what there is to
/// pick from, and the typed-search form stays docked at the bottom.
///
/// A submitted search takes over the result area only — the verdict above it
/// does not move, and Clear gives the area back. Renders from
/// `ImportSearchState` plus the form bindings and action callbacks.
struct ImportSearchPane: View {
    let state: ImportSearchState
    /// Leave the pane. `nil` for a surface that owns its own way out.
    let onBack: (() -> Void)?
    @Binding
    var activeTab: SearchTab
    @Binding
    var searchArtist: String
    @Binding
    var searchAlbum: String
    @Binding
    var searchCatalog: String
    @Binding
    var searchBarcode: String
    let onSearch: () -> Void
    /// Drop the submitted search, giving the result area back to identify.
    let onClearSearch: () -> Void
    /// Re-ask only the providers whose part of the search failed.
    let onRetrySearch: () -> Void
    let onOpenSettings: () -> Void
    /// Act on a signal — take the disc ID or barcode in or out of the run, or
    /// pick which extracted catalog number the run looks up. The state the
    /// import projection delivers is re-derived from what is left checked.
    let onToggleSignal: (BridgeSignalToggle) -> Void
    /// Start identification for a folder whose run never began. Core owns
    /// whether this starts, resumes, or does nothing.
    let onIdentify: () -> Void
    /// Run signal extraction and the lookups again.
    let onRerun: () -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (BridgeMetadataResult) -> Void

    /// The form's first field takes the keyboard on every new value. A
    /// person with nothing to pick is going to type, so an empty result area
    /// hands the cursor over, and "Search instead" hands it over again.
    @State
    private var formFocusRequest = 0

    private var verdict: FindOnlineVerdict {
        FindOnlineVerdict(
            state: state.identifyState,
            toolbar: state.signalsToolbar
        )
    }

    private var area: FindOnlineResultArea {
        FindOnlineResultArea(
            identifyState: state.identifyState,
            hasSearch: state.search != nil
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            FindOnlineHeader(
                verdict: verdict,
                toolbar: state.signalsToolbar,
                onBack: onBack,
                onIdentify: onIdentify,
                onRetry: onRerun,
                onToggleSignal: onToggleSignal,
                onRerun: onRerun,
            )
            Divider()
            identifyingChips
            errorLine
            resultArea
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            Divider()
            ImportSearchFormView(
                activeTab: $activeTab,
                searchArtist: $searchArtist,
                searchAlbum: $searchAlbum,
                searchCatalog: $searchCatalog,
                searchBarcode: $searchBarcode,
                signals: state.signals,
                focusRequest: formFocusRequest,
                onSearch: onSearch,
            )
        }
        // A person looking at an empty result area is about to type: seed the
        // Artist field from what was read off the folder and put the cursor
        // in it. Only when the fields are untouched — never over typing.
        .onChange(of: area, initial: true) { _, area in
            guard area == .nothingFound || area == .noSignals else { return }
            formFocusRequest += 1
            guard searchArtist.isEmpty, searchAlbum.isEmpty else { return }
            if let seed = state.signals?.text.freeText.first {
                searchArtist = seed
            }
        }
    }

    /// While the run is going, the signals it is looking up. A settled verdict
    /// says the same thing in one line, so the row goes with the run — and a
    /// resumed verdict, whose signals were never stored, never had one.
    @ViewBuilder
    private var identifyingChips: some View {
        if area.showsSignalChips(toolbar: state.signalsToolbar) {
            IdentifyingSignalChips(
                toolbar: state.signalsToolbar,
                onToggle: onToggleSignal,
            )
            Divider()
        }
    }

    @ViewBuilder
    private var errorLine: some View {
        if let error = state.error {
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                Text(error)
                Spacer()
            }
            .font(.caption)
            .foregroundStyle(.red)
            .padding(.horizontal, 14)
            .padding(.vertical, 6)
        }
    }

    // MARK: - Result area

    @ViewBuilder
    private var resultArea: some View {
        switch area {
        case .identifying:
            ResultSkeleton()
        case .groups:
            ReleaseGroupListView(
                groups: state.identifiedGroups,
                isImporting: state.isImporting,
                libraryStatuses: state.libraryStatuses,
                provenance: state.identifiedProvenance,
                selectedReleaseId: state.selectedReleaseId,
                loadingReleaseId: state.loadingReleaseId,
                onSelect: onSelect,
                trailing: {
                    ForEach(missingSourceNotes, id: \.self) { note in
                        MissingSourceNote(text: note)
                    }
                },
            )
        case .nothingFound:
            FindOnlineEmptyZone {
                Text("No matches.")
                    .foregroundStyle(.secondary)
                searchInstead
            }
        case .noSignals:
            FindOnlineEmptyZone {
                Text("Nothing to identify.")
                    .foregroundStyle(.secondary)
                searchInstead
            }
        case .notStarted:
            FindOnlineEmptyZone {
                IdentifyAutomaticallyButton(action: onIdentify)
            }
        case .failureLines:
            failureLines
        case .searchRun:
            searchRun
        }
    }

    /// The way out of an empty zone: the cursor goes to the form's first
    /// field. Every press is a new request, so it works after the cursor has
    /// been elsewhere.
    private var searchInstead: some View {
        Button("Search instead") { formFocusRequest += 1 }
            .buttonStyle(.link)
    }

    /// Every lookup failed, so the reasons take the place of the results.
    private var failureLines: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(state.identifyFailures, id: \.badgeLine) { failure in
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text(failure.badgeLine)
                    Spacer(minLength: 0)
                }
            }
        }
        .font(.system(size: 12.5))
        .padding(.horizontal, 18)
        .padding(.vertical, 22)
        .frame(
            maxWidth: .infinity,
            maxHeight: .infinity,
            alignment: .topLeading
        )
    }

    /// One line per source whose results the list is missing, closing it.
    private var missingSourceNotes: [String] {
        var seen: Set<BridgeMetadataSource> = []
        return state.identifyFailures.compactMap { failure in
            guard let source = failure.failedSource,
                seen.insert(source).inserted
            else { return nil }
            return String(
                localized:
                    "\(bridgeMetadataSourceName(source: source)) results are missing from this list."
            )
        }
    }

    @ViewBuilder
    private var searchRun: some View {
        if let search = state.search {
            FindOnlineSearchResults(
                search: search,
                isImporting: state.isImporting,
                libraryStatuses: state.libraryStatuses,
                selectedReleaseId: state.selectedReleaseId,
                loadingReleaseId: state.loadingReleaseId,
                onClear: onClearSearch,
                onRetry: onRetrySearch,
                onOpenSettings: onOpenSettings,
                onSelect: onSelect,
            )
        }
    }
}

#if DEBUG
    // MARK: - Previews

    extension ImportSearchPane {
        /// Preview builder — fixes the form bindings and action callbacks to
        /// inert defaults so a preview states only the situation it exercises.
        @MainActor
        static func preview(
            state: ImportSearchState,
            searchArtist: String = "",
            searchAlbum: String = "",
        ) -> ImportSearchPane {
            ImportSearchPane(
                state: state,
                onBack: {},
                activeTab: .constant(.general),
                searchArtist: .constant(searchArtist),
                searchAlbum: .constant(searchAlbum),
                searchCatalog: .constant(""),
                searchBarcode: .constant(""),
                onSearch: {},
                onClearSearch: {},
                onRetrySearch: {},
                onOpenSettings: {},
                onToggleSignal: { _ in },
                onIdentify: {},
                onRerun: {},
                onSelect: { _ in },
            )
        }
    }

    #Preview("Find online — identifying") {
        ImportSearchPane.preview(state: PreviewData.searchStateTriangulating)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — found, cross-linked") {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — signals named different releases") {
        ImportSearchPane.preview(state: PreviewData.searchStateDisagreement)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — nothing found") {
        ImportSearchPane.preview(state: PreviewData.searchStateNotFound)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — no signals") {
        ImportSearchPane.preview(state: PreviewData.searchStateNoSignals)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — source failure, partial results") {
        ImportSearchPane.preview(state: PreviewData.searchStateSourceFailure)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — every source failed") {
        ImportSearchPane.preview(state: PreviewData.searchStateAllSourcesFailed)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — not identified") {
        ImportSearchPane.preview(state: PreviewData.searchStateIdle)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — searching") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateSearching,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title One",
        )
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
    }

    #Preview("Find online — search results") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateManual,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title One",
        )
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
    }

    #Preview("Find online — a searched source dropped") {
        ImportSearchPane.preview(state: PreviewData.searchStateSearchFailed)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — search matched nothing") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateSearchEmpty,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title",
        )
        .frame(width: 900, height: 620)
        .importPreviewEnvironment()
    }
#endif
