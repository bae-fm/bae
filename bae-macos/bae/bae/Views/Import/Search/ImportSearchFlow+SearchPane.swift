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
        let state = searchPaneState(candidate: input.candidate, input: input)
        let progress = SoleMatchProgress(
            state: state,
            candidate: input.candidate
        )

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
                let choices = input.candidate.lookupChoices.choosing(value)
                switch input.candidate.source {
                // A library release has no candidate row to store a choice on,
                // so the sheet's session holds it and the restarted run reads
                // it from there.
                case .releaseReIdentify(let releaseId):
                    importStore.mutateCandidate(forKey: key) {
                        $0.lookupChoices = choices
                    }
                    services.importer.autoIdentifyRelease(
                        key,
                        releaseId,
                        choices
                    )
                // Core stores it and starts the run; the candidate's next
                // detail carries it back.
                case .folder:
                    Task { @MainActor in
                        do {
                            try await services.importer
                                .setCandidateLookupChoices(key, choices)
                        }
                        catch is CancellationError {}
                        catch {
                            guard let line = error.displayLine else { return }
                            importStore.recordPaneError(
                                String(
                                    localized:
                                        "Couldn't change what identification looks up: \(line)"
                                ),
                                forKey: key
                            )
                        }
                    }
                }
            },
            onIdentify: {
                services.importer.identifyForExplicitLookup(key)
            },
            onRetryFailed: {
                services.importer.retryFailedIdentifyForCandidate(key)
            },
            onSelect: onSelect,
        )
        // A sole match core picks on its own leaves nothing here to do: the
        // draft already carries the claim the person's click would make. Leave
        // Find online the way a pick by hand leaves it — `applyMetadata`'s
        // `onConfirmed` calls the same way out.
        //
        // Only on the transition, and inside `.id(key)` so it is this
        // candidate's: a candidate whose draft already carried the pick when
        // Find online opened stays open, because being here is then something
        // the person asked for.
        //
        // The re-identify sheet is untouched twice over — it hands the pane no
        // way back, and its candidate has no stored draft for a pick to land
        // on, so its progress never leaves `nil`.
        .onChange(of: progress) { was, now in
            if now.followedCorePick(after: was) {
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

    /// What core is claiming for this candidate on its own, and what its
    /// draft has come to claim. Both describe the candidate as it stands: the
    /// first is the pick of the one pressing a run matched, held while core
    /// commits it; the second is the pick the stored draft was read from. Core
    /// writes the verdict and the draft in one row, so the two become the same
    /// value the instant that write lands.
    struct SoleMatchProgress: Equatable {
        /// The pick core is committing on its own, while it commits it.
        let committing: BridgeMetadataProvenance?
        /// The pick the candidate's stored draft was read from.
        let applied: BridgeMetadataProvenance?

        init(
            committing: BridgeMetadataProvenance?,
            applied: BridgeMetadataProvenance?
        ) {
            self.committing = committing
            self.applied = applied
        }

        init(state: ImportSearchState, candidate: Candidate) {
            self.init(
                committing: state.finalizingPressing?.provenance,
                applied: candidate.metadataProvenance
            )
        }

        /// Whether this value follows `previous` by core's own pick landing on
        /// the draft: what core was committing is now what the draft carries,
        /// and it was not already.
        func followedCorePick(after previous: SoleMatchProgress) -> Bool {
            guard let committing = previous.committing else { return false }
            return applied == committing && previous.applied != committing
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

    /// The pane's read-only state snapshot from the candidate, plus the
    /// open-confirm selection the pane renders against.
    @MainActor
    private static func searchPaneState(
        candidate: Candidate,
        input: SearchPaneInput
    ) -> ImportSearchState {
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
            loadingReleaseId: candidate.loadingReleaseId,
            releaseSelectionFailure: candidate.releaseSelectionFailure,
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
