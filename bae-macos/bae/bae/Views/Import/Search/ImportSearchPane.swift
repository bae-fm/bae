import BaeKit
import SwiftUI

/// Find online: two sections, one open at a time. AUTOMATIC lays the
/// identify run out as a ledger and lists what it matched; SEARCH holds the
/// typed-search form with what it turned up beneath. Each section's results
/// render under the header that produced them, and each header carries one
/// glyph saying how its part is going, open or collapsed.
///
/// Opening SEARCH collapses AUTOMATIC without cancelling its run: the run
/// carries on behind its header, and its results wait behind the glyph.
/// Renders from `ImportSearchState` plus the form bindings and action
/// callbacks.
struct ImportSearchPane: View {
    let state: ImportSearchState
    /// Leave the pane. `nil` for a surface that owns its own way out.
    let onBack: (() -> Void)?
    /// The typed-search form as the candidate stores it.
    let form: CandidateSearchState
    /// The form as the person left it, to store with the candidate.
    let onCommitForm: (CandidateSearchState) -> Void
    /// Search with the form as it stands.
    let onSearch: (CandidateSearchState) -> Void
    /// Re-ask only the providers whose part of the search failed.
    let onRetrySearch: () -> Void
    /// Open Settings on the Discogs page — what the not-configured bar offers.
    let onOpenSettings: () -> Void
    /// Take a catalog number in or out of the run. Core re-derives the state
    /// the import projection delivers from what is chosen.
    let onToggleCatalog: (String) -> Void
    /// Count a catalog number the folder states, or stop counting it: the
    /// same answers, ranked by what the folder is taken to state about them.
    let onToggleCatalogAgreement: (String) -> Void
    /// Re-ask only the lookups that failed, keeping what the others found.
    let onRetryFailed: () -> Void
    /// A pressing row was picked — the flow opens the docked confirm pane.
    let onSelect: (Pressing) -> Void

    /// Whether Discogs can be asked at all is core's answer, carried on the
    /// config the app observes: adding a token in Settings takes the notice
    /// away while the pane is open.
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore

    /// Which section is open. Seeded from the section the entry that opened
    /// the pane asked for — a candidate that already has a search submitted
    /// opens on SEARCH, where its results are. The pane is built afresh every
    /// time it is opened, so the seed lands every time.
    @State
    private var openSection: FindOnlineSection

    /// `initialSection` seeds `openSection` and is not kept: which section is
    /// open afterwards is the person's, not the caller's.
    init(
        state: ImportSearchState,
        onBack: (() -> Void)?,
        form: CandidateSearchState,
        onCommitForm: @escaping (CandidateSearchState) -> Void,
        onSearch: @escaping (CandidateSearchState) -> Void,
        onRetrySearch: @escaping () -> Void,
        onOpenSettings: @escaping () -> Void,
        onToggleCatalog: @escaping (String) -> Void,
        onToggleCatalogAgreement: @escaping (String) -> Void,
        initialSection: FindOnlineSection,
        onRetryFailed: @escaping () -> Void,
        onSelect: @escaping (Pressing) -> Void
    ) {
        self.state = state
        self.onBack = onBack
        self.form = form
        self.onCommitForm = onCommitForm
        self.onSearch = onSearch
        self.onRetrySearch = onRetrySearch
        self.onOpenSettings = onOpenSettings
        self.onToggleCatalog = onToggleCatalog
        self.onToggleCatalogAgreement = onToggleCatalogAgreement
        self.onRetryFailed = onRetryFailed
        self.onSelect = onSelect
        _openSection = State(initialValue: initialSection)
    }

    /// The form's first field takes the keyboard on every new value: Search
    /// manually hands the cursor over, and does so again after it has been
    /// elsewhere.
    @State
    private var formFocusRequest = 0
    /// Whether the releases agreement narrowed out are showing. Closed on
    /// arrival: the matches are the answer.
    @State
    private var narrowedOutExpanded = false

    var body: some View {
        VStack(spacing: 0) {
            FindOnlineHeader(onBack: onBack)
            Divider()
            discogsBar
            errorLine
            FindOnlineSectionHeader(
                section: .automatic,
                isOpen: openSection == .automatic,
                glyph: FindOnlineSectionGlyph(
                    identifyState: state.identifyState
                ),
                onOpen: { openSection = .automatic }
            )
            if openSection == .automatic {
                FindOnlineAutomaticSection(
                    state: state,
                    onOpenSettings: onOpenSettings,
                    onToggleCatalog: onToggleCatalog,
                    onToggleCatalogAgreement: onToggleCatalogAgreement,
                    onRetryFailed: onRetryFailed,
                    onSelect: onSelect,
                    onSearchManually: searchManually,
                    narrowedOutExpanded: $narrowedOutExpanded,
                )
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .top
                )
            }
            Divider()
            FindOnlineSectionHeader(
                section: .search,
                isOpen: openSection == .search,
                glyph: FindOnlineSectionGlyph(search: state.search),
                onOpen: { openSection = .search }
            )
            if openSection == .search {
                searchContent
                    .frame(
                        maxWidth: .infinity,
                        maxHeight: .infinity,
                        alignment: .top
                    )
            }
        }
        .onAppear {
            if state.search != nil {
                openSection = .search
            }
        }
    }

    /// Above both sections, because neither of them asked Discogs. Gone for
    /// the rest of the session once put away, in every candidate's pane.
    @ViewBuilder
    private var discogsBar: some View {
        if !configStore.config.discogsUsable, !uiStore.discogsNoticeDismissed {
            FindOnlineDiscogsBar(
                onOpenSettings: onOpenSettings,
                onDismiss: { uiStore.dismissDiscogsNotice() }
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
            Divider()
        }
    }

    /// Open SEARCH with the cursor in its first field, seeded from what was
    /// read off the folder — only when the fields are untouched, never over
    /// typing.
    private func searchManually() {
        openSection = .search
        formFocusRequest += 1
        guard form.searchArtist.isEmpty, form.searchAlbum.isEmpty else {
            return
        }
        if let seed = state.signals?.text.freeText.first {
            var seeded = form
            seeded.searchArtist = seed
            onCommitForm(seeded)
        }
    }

    // MARK: - SEARCH

    /// The form, with what it turned up beneath it.
    private var searchContent: some View {
        VStack(spacing: 0) {
            ImportSearchFormView(
                form: form,
                onCommit: onCommitForm,
                signals: state.signals,
                focusRequest: formFocusRequest,
                onSearch: onSearch,
            )
            if let search = state.search {
                Divider()
                    .padding(.horizontal, 14)
                FindOnlineSearchResults(
                    search: search,
                    isImporting: state.isImporting,
                    libraryStatuses: state.libraryStatuses,
                    selectedReleaseId: state.selectedReleaseId,
                    loadingReleaseId: state.loadingReleaseId,
                    releaseSelectionFailure: state.releaseSelectionFailure,
                    onRetry: onRetrySearch,
                    onSelect: onSelect,
                )
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    extension ImportSearchPane {
        /// Preview builder — fixes the form bindings and action callbacks to
        /// inert defaults so a preview states only the situation it exercises.
        /// A test that presses one of them passes it in.
        @MainActor
        static func preview(
            state: ImportSearchState,
            searchArtist: String = "",
            searchAlbum: String = "",
            initialSection: FindOnlineSection = .automatic,
            onRetryFailed: @escaping () -> Void = {},
        ) -> ImportSearchPane {
            ImportSearchPane(
                state: state,
                onBack: {},
                form: CandidateSearchState(
                    searchArtist: searchArtist,
                    searchAlbum: searchAlbum
                ),
                onCommitForm: { _ in },
                onSearch: { _ in },
                onRetrySearch: {},
                onOpenSettings: {},
                onToggleCatalog: { _ in },
                onToggleCatalogAgreement: { _ in },
                initialSection: initialSection,
                onRetryFailed: onRetryFailed,
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

    #Preview("Find online — releases the agreement left out") {
        ImportSearchPane.preview(state: PreviewData.searchStateNarrowedOut)
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

    #Preview("Find online — catalog numbers to activate") {
        ImportSearchPane.preview(state: PreviewData.searchStateAwaitingCatalog)
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

    #Preview("Find online — a failure with no ledger") {
        ImportSearchPane.preview(state: PreviewData.searchStateFailedWithoutRun)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — not identified") {
        ImportSearchPane.preview(state: PreviewData.searchStateIdle)
            .frame(width: 900, height: 620)
            .importPreviewEnvironment()
    }

    #Preview("Find online — finalizing a sole match") {
        ImportSearchPane.preview(state: PreviewData.searchStateFinalizing)
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

    #Preview("Find online — Discogs not configured") {
        ImportSearchPane.preview(
            state: PreviewData.searchStateManual,
            searchArtist: "Artist Name",
            searchAlbum: "Album Title One",
        )
        .environment(
            PreviewData.makeConfigStore(
                libraryFullWidth: false,
                discogsUsable: false
            )
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
