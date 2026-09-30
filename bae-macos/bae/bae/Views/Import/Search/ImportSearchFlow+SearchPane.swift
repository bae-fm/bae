import BaeKit
import SwiftUI

extension ImportSearchFlow {
    // MARK: - Shared search pane builder

    /// The import-flow services a search pane drives: search and identify on
    /// `importer`, candidate state on `importStore`. The opening surface owns
    /// what selecting a result does.
    struct ImportServices {
        let importer: Importer
        let importStore: ImportStore
    }

    /// Which candidate a search pane renders, and the selection state it shows:
    /// `selectedReleaseId` is the pressing whose confirm pane is open, so its
    /// row renders selected.
    struct SearchPaneInput {
        let candidate: Candidate
        let key: String
        let selectedReleaseId: String?
        /// What is in flight for this key: the run whose verdict and ledger
        /// the pane shows. `nil` when nothing is running for it.
        let runtime: BridgeCandidateRuntimeSnapshot?
        /// Which section is open: a folder candidate's as core's session
        /// says, the re-identify sheet's own.
        let openSection: BridgeFindOnlineSection
        /// Open one section: for a folder candidate, a move core makes.
        let onOpenSection: (BridgeFindOnlineSection) -> Void
        /// Keep the folder's own draft over what its lookup offered; `nil`
        /// for a library release, which has no folder draft to keep.
        let onKeepOwnDraft: (() -> Void)?
        /// Link the folder to the album the offered pressings are of, its
        /// pressing unknown; `nil` for a library release, which has no folder
        /// to link.
        let onLinkSharedAlbum: (() -> Void)?
        /// Take the candidate off the identification queue and leave the
        /// pane; `nil` for a library release, which is never queued.
        let onCancelIdentification: (() -> Void)?
        /// What extraction has found for this key so far, feeding the form's
        /// suggestion pools and its scanning indicator. `nil` before
        /// extraction has reported any, and for a candidate whose run settled
        /// in an earlier session — the stored row answers for that one.
        let liveSignals: Signals?
    }

    /// `onSelect` owns what picking a pressing means for the surface that
    /// opened the pane. Import applies it to the candidate draft; re-identify
    /// keeps it selected until its own footer commits the library release.
    ///
    /// `onBack` is the pane's way out. The re-identify sheet passes `nil`: it
    /// closes rather than going back to anything.
    @MainActor
    @ViewBuilder
    static func buildSearchPane(
        services: ImportServices,
        input: SearchPaneInput,
        openSettings: @escaping () -> Void,
        onBack: (() -> Void)?,
        onSelect: @escaping (Pressing) -> Void
    ) -> some View {
        let key = input.key
        let importStore = services.importStore
        let state = searchPaneState(input: input, importStore: importStore)

        ImportSearchPane(
            state: state,
            onBack: onBack,
            onCancelIdentification: input.onCancelIdentification,
            form: input.candidate.search,
            onCommitForm: { form in
                importStore.commitSearchForm(form, forKey: key)
            },
            onSearch: { form in
                startSearch(
                    importer: services.importer,
                    importStore: importStore,
                    key: key,
                    form: form
                )
            },
            onRetrySearch: { services.importer.retryCandidateSearch(key) },
            onOpenSettings: openSettings,
            // The chip acts on one identifier; that change goes to core, and
            // the run that reads the changed choices starts from there.
            onToggleLookup: { toggle in
                toggleLookup(toggle, services: services, input: input)
            },
            onToggleCatalogAgreement: { value in
                toggleCatalogAgreement(value, services: services, input: input)
            },
            // Re-asking what failed is asking for the run again: it reads
            // its inputs afresh, and the response cache answers the lookups
            // that had already succeeded. Where those inputs live is what
            // differs — the same split `onToggleLookup` makes.
            onRetryFailed: {
                rerunIdentification(services: services, input: input)
            },
            // The words go to core as a change to what the candidate's
            // identification searches by, and the run that reads them starts
            // from there.
            onEditTitleSearch: { album, artist in
                editTitleSearch(
                    album: album,
                    artist: artist,
                    services: services,
                    input: input
                )
            },
            onSelect: onSelect,
            onKeepOwnDraft: input.onKeepOwnDraft,
            onLinkSharedAlbum: input.onLinkSharedAlbum,
            openSection: input.openSection,
            onOpenSection: input.onOpenSection,
        )
        // The pane's own view state — the form's focus, the set-aside rows —
        // is this candidate's: another candidate's pane starts on its own.
        .id(key)
        // Every release the pane is offering is watched for library membership
        // while it is open: each provider lands its own part, so the set they
        // amount to changes as the run advances.
        .task(id: releaseStatusKeys(state: state)) {
            importStore.refreshLibraryStatusSubscriptions(
                importer: services.importer,
                key: key,
                desired: releaseStatusKeys(state: state)
            )
        }
    }

    /// Take one identifier in or out of the run: the disc ID, one of the
    /// candidate's barcodes, or one of its catalog numbers.
    @MainActor
    private static func toggleLookup(
        _ toggle: LookupToggle,
        services: ImportServices,
        input: SearchPaneInput
    ) {
        editLookupChoices(
            toggle.edit,
            services: services,
            input: input
        )
    }

    /// Search by the words the person left in the title chip, in place of
    /// what the draft calls the release.
    @MainActor
    private static func editTitleSearch(
        album: String,
        artist: String,
        services: ImportServices,
        input: SearchPaneInput
    ) {
        editLookupChoices(
            .searchBy(album: album, artist: artist),
            services: services,
            input: input
        )
    }

    /// Count or stop counting a catalog number the folder states as an
    /// agreement.
    @MainActor
    private static func toggleCatalogAgreement(
        _ value: String,
        services: ImportServices,
        input: SearchPaneInput
    ) {
        editLookupChoices(
            .toggleDiscounted(number: value),
            services: services,
            input: input
        )
    }

    /// Run identification again from the choices core holds: a folder's stored
    /// ones, or a library release's for its session.
    @MainActor
    private static func rerunIdentification(
        services: ImportServices,
        input: SearchPaneInput
    ) {
        switch input.candidate.source {
        case .releaseReIdentify(let releaseId):
            services.importer.autoIdentifyRelease(input.key, releaseId)
        case .folder:
            services.importer.rerunIdentifyForCandidate(input.key)
        }
    }

    /// Send one change to what this candidate's identification asks about
    /// and counts; core makes it to the choices it holds and starts the run
    /// that reads them. A folder's choices are stored with it; a library
    /// release's are held for its re-identify session, and each change runs
    /// it again. Striking a number out asks nothing of a folder's providers;
    /// the same answers come back ranked by it.
    @MainActor
    private static func editLookupChoices(
        _ edit: BridgeLookupChoiceEdit,
        services: ImportServices,
        input: SearchPaneInput
    ) {
        let key = input.key
        let importStore = services.importStore
        switch input.candidate.source {
        case .releaseReIdentify(let releaseId):
            services.importer.editReleaseLookupChoices(key, releaseId, edit)
        case .folder:
            // Core states a failure on the pane; one it could not store is
            // told here.
            Task { @MainActor in
                do {
                    _ = try await services.importer.editCandidateLookupChoices(
                        key,
                        edit
                    )
                }
                catch is CancellationError {}
                catch { importStore.reportFailure(error) }
            }
        }
    }

    /// Every release the pane offers — the identify verdict's pressings and
    /// the typed search's — as the keys a library-membership subscription
    /// takes. A pressing carries one release per source and a pick claims them
    /// all, so each is separately watched.
    @MainActor
    static func releaseStatusKeys(
        state: ImportSearchState
    ) -> Set<BridgeLibraryCheck> {
        let searched = (state.search?.groups ?? [])
            .map(ReleaseGroup.init(bridge:))
        return Set(
            (state.identifiedGroups + searched)
                .flatMap(\.pressings)
                .flatMap(\.releases)
                .map { release in
                    BridgeLibraryCheck(
                        source: release.source,
                        releaseId: release.releaseId,
                        sourceGroupId: release.sourceGroupId
                    )
                }
        )
    }

    /// The pane's read-only state snapshot from the candidate, the pick the
    /// store is reading for it, and the open-confirm selection the pane
    /// renders against.
    @MainActor
    private static func searchPaneState(
        input: SearchPaneInput,
        importStore: ImportStore
    ) -> ImportSearchState {
        let candidate = input.candidate
        let identifyState = shownIdentifyState(
            resumed: candidate.resumedIdentifyState,
            runtime: input.runtime
        )
        // Core's own statuses are what it checked when each verdict or
        // provider landed; a live subscription's value is fresher, so it wins.
        var libraryStatuses = identifyState.libraryStatuses
        libraryStatuses.merge(input.runtime?.search?.libraryStatuses ?? [:]) {
            _,
            searched in searched
        }
        libraryStatuses.merge(candidate.libraryStatuses) { _, live in live }
        return ImportSearchState(
            identifyState: identifyState,
            error: candidate.error,
            search: input.runtime?.search,
            selectedReleaseId: input.selectedReleaseId,
            loadingReleaseId: importStore.loadingReleaseId(forKey: input.key),
            releaseSelectionFailure: importStore.releaseSelectionFailure(
                forKey: input.key
            ),
            isImporting: isImporting(candidate),
            isFinalizing: candidate.live?.identification == .finalizing,
            libraryStatuses: libraryStatuses,
            // The run in flight knows more than the last stored answer does,
            // and for a re-identify key — which has no row at all — it is the
            // only answer.
            signals: input.liveSignals ?? candidate.settledSignals,
            needsYou: candidate.live?.standing?.needsYou,
        )
    }

}
