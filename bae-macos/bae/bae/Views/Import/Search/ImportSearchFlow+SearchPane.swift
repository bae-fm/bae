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
            // The chip acts on one number; what goes back is the whole value
            // of what this candidate's identification asks about, and the run
            // that reads it starts from there.
            onToggleCatalog: { value in
                toggleCatalogLookup(value, services: services, input: input)
            },
            onToggleCatalogAgreement: { value in
                toggleCatalogAgreement(value, services: services, input: input)
            },
            onIdentify: {
                services.importer.identifyForExplicitLookup(key)
            },
            // Re-asking what failed is asking for the run again: it reads
            // its inputs afresh, and the response cache answers the lookups
            // that had already succeeded. Where those inputs live is what
            // differs — the same split `onToggleCatalog` makes.
            onRetryFailed: {
                rerunIdentification(services: services, input: input)
            },
            onSelect: onSelect,
        )
        // The draft saying identification wrote it leaves nothing here to do:
        // it already carries the claim the person's click would make. Leave
        // Find online the way a pick by hand leaves it — that one goes back to
        // the draft through core when its own read lands.
        //
        // Only on the transition, and inside `.id(key)` so it is this
        // candidate's: a candidate whose draft identification had already
        // written when Find online opened stays open, because being here is
        // then something the person asked for.
        //
        // The re-identify sheet is untouched twice over — it hands the pane no
        // way back, and its candidate has no row to write an author on, so its
        // author never leaves `.nobody`.
        .onChange(of: input.candidate.metadataAuthor) { _, now in
            if now == .identification {
                onBack?()
            }
        }
        // Which section is open is this candidate's: another candidate's
        // pane starts on its own.
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

    /// Choose or unchoose a catalog number for lookup.
    @MainActor
    private static func toggleCatalogLookup(
        _ value: String,
        services: ImportServices,
        input: SearchPaneInput
    ) {
        writeLookupChoices(
            input.candidate.lookupChoices.choosing(value),
            services: services,
            input: input,
            failure: { line in
                String(
                    localized:
                        "Couldn't change what identification looks up: \(line)"
                )
            }
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
        writeLookupChoices(
            input.candidate.lookupChoices.discounting(value),
            services: services,
            input: input,
            failure: { line in
                String(
                    localized:
                        "Couldn't change what counts as an agreement: \(line)"
                )
            }
        )
    }

    /// Run identification again from the candidate's stored choices. A library
    /// release has no candidate row, so the sheet restarts its own run with the
    /// choices it holds.
    @MainActor
    private static func rerunIdentification(
        services: ImportServices,
        input: SearchPaneInput
    ) {
        switch input.candidate.source {
        case .releaseReIdentify(let releaseId):
            services.importer.autoIdentifyRelease(
                input.key,
                releaseId,
                input.candidate.lookupChoices
            )
        case .folder:
            services.importer.rerunIdentifyForCandidate(input.key)
        }
    }

    /// Store the whole value of what this candidate's identification asks
    /// about and counts, and let the run that reads it start from there.
    ///
    /// A library release has no candidate row to store a choice on, so the
    /// re-identify sheet's session holds it and the restarted run reads it
    /// from there. A folder's choices are core's: it stores them, starts a run
    /// when what is looked up changed, and the candidate's next detail carries
    /// them back. Striking a number out asks nothing of the providers; the
    /// same answers come back ranked by it.
    @MainActor
    private static func writeLookupChoices(
        _ choices: BridgeLookupChoices,
        services: ImportServices,
        input: SearchPaneInput,
        failure: @escaping (String) -> String
    ) {
        let key = input.key
        let importStore = services.importStore
        switch input.candidate.source {
        case .releaseReIdentify(let releaseId):
            importStore.mutateCandidate(forKey: key) {
                $0.lookupChoices = choices
            }
            services.importer.autoIdentifyRelease(key, releaseId, choices)
        case .folder:
            Task { @MainActor in
                do {
                    try await services.importer.setCandidateLookupChoices(
                        key,
                        choices
                    )
                }
                catch is CancellationError {}
                catch {
                    guard let line = error.displayLine else { return }
                    importStore.recordPaneError(failure(line), forKey: key)
                }
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
    ) -> Set<ReleaseLibraryStatusSubscriptionKey> {
        let searched = (state.search?.groups ?? [])
            .map(ReleaseGroup.init(bridge:))
        return Set(
            (state.identifiedGroups + searched)
                .flatMap(\.pressings)
                .flatMap(\.releases)
                .map { release in
                    ReleaseLibraryStatusSubscriptionKey(
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
            isFinalizing: candidate.row?.placement
                == .identification(status: .finalizing),
            libraryStatuses: libraryStatuses,
            // The run in flight knows more than the last stored answer does,
            // and for a re-identify key — which has no row at all — it is the
            // only answer.
            signals: input.liveSignals ?? candidate.settledSignals,
            filePaths: Dictionary(
                candidate.files.files.map { ($0.file.name, $0.file.localPath) },
                uniquingKeysWith: { first, _ in first }
            ),
        )
    }

}
