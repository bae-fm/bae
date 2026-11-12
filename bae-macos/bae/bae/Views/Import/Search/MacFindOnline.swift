import BaeKit
import SwiftUI

/// Find online, in two sections with one open at a time: Automatic, the
/// identify run and its matches, and Search, the typed-search form and its
/// results. Collapsing Automatic does not cancel its run.
struct MacFindOnline: View {
    let state: ImportSearchState
    /// Leave the pane. `nil` for a surface that owns its own way out.
    let onBack: (() -> Void)?
    /// Take the candidate off the identification queue and leave the pane as
    /// `onBack` does; `nil` for a surface whose candidate is never queued.
    let onCancelIdentification: (() -> Void)?
    /// The typed-search form as the candidate stores it.
    let form: CandidateSearchState
    /// The form as the person left it, to store with the candidate.
    let onCommitForm: (CandidateSearchState) -> Void
    /// Search with the form as it stands.
    let onSearch: (CandidateSearchState) -> Void
    /// Re-ask only the providers whose part of the search failed.
    let onRetrySearch: () -> Void
    /// Open settings on the Discogs page.
    let onOpenSettings: () -> Void
    /// Leave an identifier (Disc ID, barcode, catalog number) out of the run,
    /// or ask about it again.
    let onToggleLookup: (LookupToggle) -> Void
    /// Count a catalog number the folder states, or stop counting it.
    let onToggleCatalogAgreement: (String) -> Void
    /// Re-ask only the lookups that failed, keeping what the others found.
    let onRetryFailed: () -> Void
    /// Search by the words the person left in the title chip.
    let onEditTitleSearch: (_ album: String, _ artist: String) -> Void
    /// A pressing row was picked.
    let onSelect: (Pressing) -> Void
    /// Keep the folder's own draft over what the lookup offered; `nil` for a
    /// surface with no draft of its own to keep.
    let onKeepOwnDraft: (() -> Void)?
    /// Link the folder to the album the offered pressings are of, its
    /// pressing unknown; `nil` for a surface with no folder to link.
    let onLinkSharedAlbum: (() -> Void)?
    /// Which section is open, as the candidate's session says.
    let openSection: BridgeFindOnlineSection
    /// Open one section.
    let onOpenSection: (BridgeFindOnlineSection) -> Void

    /// Whether the Discogs bar shows above both sections: while Discogs
    /// can't be asked, until the person puts it away for the session.
    let showsDiscogsBar: Bool
    /// Put the Discogs bar away for the session.
    let onDismissDiscogsBar: () -> Void

    init(
        state: ImportSearchState,
        onBack: (() -> Void)?,
        onCancelIdentification: (() -> Void)?,
        form: CandidateSearchState,
        onCommitForm: @escaping (CandidateSearchState) -> Void,
        onSearch: @escaping (CandidateSearchState) -> Void,
        onRetrySearch: @escaping () -> Void,
        onOpenSettings: @escaping () -> Void,
        onToggleLookup: @escaping (LookupToggle) -> Void,
        onToggleCatalogAgreement: @escaping (String) -> Void,
        onRetryFailed: @escaping () -> Void,
        onEditTitleSearch:
            @escaping (_ album: String, _ artist: String) -> Void,
        onSelect: @escaping (Pressing) -> Void,
        onKeepOwnDraft: (() -> Void)?,
        onLinkSharedAlbum: (() -> Void)?,
        openSection: BridgeFindOnlineSection,
        onOpenSection: @escaping (BridgeFindOnlineSection) -> Void,
        showsDiscogsBar: Bool,
        onDismissDiscogsBar: @escaping () -> Void
    ) {
        self.state = state
        self.onBack = onBack
        self.onCancelIdentification = onCancelIdentification
        self.form = form
        self.onCommitForm = onCommitForm
        self.onSearch = onSearch
        self.onRetrySearch = onRetrySearch
        self.onOpenSettings = onOpenSettings
        self.onToggleLookup = onToggleLookup
        self.onToggleCatalogAgreement = onToggleCatalogAgreement
        self.onRetryFailed = onRetryFailed
        self.onEditTitleSearch = onEditTitleSearch
        self.onSelect = onSelect
        self.onKeepOwnDraft = onKeepOwnDraft
        self.onLinkSharedAlbum = onLinkSharedAlbum
        self.openSection = openSection
        self.onOpenSection = onOpenSection
        self.showsDiscogsBar = showsDiscogsBar
        self.onDismissDiscogsBar = onDismissDiscogsBar
    }

    /// Each new value gives the form's first field the keyboard.
    @State
    private var formFocusRequest = 0
    /// Whether the releases agreement narrowed out are shown; closed at first.
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
                onOpen: { onOpenSection(.automatic) }
            )
            if openSection == .automatic {
                FindOnlineAutomaticSection(
                    state: state,
                    onOpenSettings: onOpenSettings,
                    onToggleLookup: onToggleLookup,
                    onToggleCatalogAgreement: onToggleCatalogAgreement,
                    onRetryFailed: onRetryFailed,
                    onEditTitleSearch: onEditTitleSearch,
                    onSelect: onSelect,
                    onSearchManually: searchManually,
                    onCancelIdentification: onCancelIdentification,
                    onKeepOwnDraft: onKeepOwnDraft,
                    onLinkSharedAlbum: onLinkSharedAlbum,
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
                onOpen: { onOpenSection(.search) }
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
    }

    /// Above both sections, since neither asked Discogs; once dismissed it
    /// stays hidden for the session.
    @ViewBuilder
    private var discogsBar: some View {
        if showsDiscogsBar {
            FindOnlineDiscogsBar(
                onOpenSettings: onOpenSettings,
                onDismiss: onDismissDiscogsBar
            )
        }
    }

    /// Whether the bar shows: while Discogs can't be asked, until the
    /// person puts it away for the session.
    static func showsDiscogsBar(_ config: Config, dismissed: Bool) -> Bool {
        !config.discogsUsable && !dismissed
    }

    @ViewBuilder
    private var errorLine: some View {
        if let error = state.error {
            ErrorText(error)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, ThemeSpace.group)
                .padding(.vertical, ThemeSpace.compact)
            Divider()
        }
    }

    /// Open Search with the cursor in its first field, seeding Artist from the
    /// folder's text when Artist and Album are both empty.
    private func searchManually() {
        onOpenSection(.search)
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
                    .padding(.horizontal, ThemeSpace.group)
                FindOnlineSearchResults(
                    search: search,
                    isImporting: state.isImporting,
                    libraryStatuses: state.libraryStatuses,
                    selectedReleaseId: state.selectedReleaseId,
                    loadingPick: state.loadingPick,
                    releaseSelectionFailure: state.releaseSelectionFailure,
                    onRetry: onRetrySearch,
                    onSelect: onSelect,
                )
            }
        }
    }
}

#if DEBUG
    // MARK: - Shot staging

    extension MacFindOnline {
        /// A pane with inert form bindings and callbacks, so a shot states
        /// only its situation.
        @MainActor
        static func preview(
            state: ImportSearchState,
            searchArtist: String = "",
            searchAlbum: String = "",
            openSection: BridgeFindOnlineSection = .automatic,
            onRetryFailed: @escaping () -> Void = {},
            showsDiscogsBar: Bool = false,
        ) -> MacFindOnline {
            MacFindOnline(
                state: state,
                onBack: {},
                onCancelIdentification: {},
                form: CandidateSearchState(
                    searchArtist: searchArtist,
                    searchAlbum: searchAlbum
                ),
                onCommitForm: { _ in },
                onSearch: { _ in },
                onRetrySearch: {},
                onOpenSettings: {},
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: onRetryFailed,
                onEditTitleSearch: { _, _ in },
                onSelect: { _ in },
                onKeepOwnDraft: {},
                onLinkSharedAlbum: {},
                openSection: openSection,
                onOpenSection: { _ in },
                showsDiscogsBar: showsDiscogsBar,
                onDismissDiscogsBar: {}
            )
        }
    }

#endif
