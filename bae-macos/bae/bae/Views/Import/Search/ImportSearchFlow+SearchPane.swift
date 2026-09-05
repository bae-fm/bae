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
        /// What is in flight for this key: the run whose verdict and signals
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
            onClearSearch: { services.importer.clearCandidateSearch(key) },
            onRetrySearch: { services.importer.retryCandidateSearch(key) },
            onOpenSettings: openSettings,
            onToggleSignal: { signal in
                services.importer.toggleSignalForCandidate(key, signal)
            },
            onIdentify: {
                services.importer.identifyForExplicitLookup(key)
            },
            onRerun: { services.importer.rerunIdentifyForCandidate(key) },
            onRetryFailed: {
                services.importer.retryFailedIdentifyForCandidate(key)
            },
            onSelect: onSelect,
        )
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
            isImporting: isImporting(candidate),
            isFinalizing: candidate.row?.placement
                == .identification(status: .finalizing),
            libraryStatuses: libraryStatuses,
            // The run in flight knows more than the last stored answer does,
            // and for a re-identify key — which has no row at all — it is the
            // only answer.
            signals: input.liveSignals ?? candidate.settledSignals,
            signalsToolbar: input.runtime?.signalsToolbar
                ?? BridgeSignalsToolbar(signals: []),
        )
    }

}
