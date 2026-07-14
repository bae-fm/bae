import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("ImportSearchFlow")

enum ImportSearchFlow {
    // MARK: - Bindings that read/write Candidate.search on the store

    @MainActor
    static func makeActiveTabBinding(
        importStore: ImportStore,
        key: String,
        candidate: Candidate
    ) -> Binding<SearchTab> {
        Binding(
            get: { candidate.search.activeTab },
            set: { newValue in
                importStore.mutateCandidate(forKey: key) {
                    $0.search.activeTab = newValue
                }
            },
        )
    }

    @MainActor
    static func makeActiveSourceBinding(
        importStore: ImportStore,
        key: String,
        candidate: Candidate
    ) -> Binding<BridgeMetadataSource> {
        Binding(
            get: { candidate.search.activeSource },
            set: { newValue in
                importStore.mutateCandidate(forKey: key) {
                    $0.search.activeSource = newValue
                }
            },
        )
    }

    @MainActor
    static func makeSearchFieldBinding(
        importStore: ImportStore,
        key: String,
        candidate: Candidate,
        field: WritableKeyPath<CandidateSearchState, String>
    ) -> Binding<String> {
        Binding(
            get: { candidate.search[keyPath: field] },
            set: { newValue in
                importStore.mutateCandidate(forKey: key) {
                    $0.search[keyPath: field] = newValue
                }
            },
        )
    }

    // MARK: - Search dispatch

    @MainActor
    static func dispatchSearch(
        importer: Importer,
        importStore: ImportStore,
        key: String
    ) {
        guard let snapshot = importStore.candidate(forKey: key) else {
            return
        }
        let capturedTab = snapshot.search.activeTab
        let capturedSource = snapshot.search.activeSource
        let query = searchQuery(from: snapshot.search)

        importStore.mutateCandidate(forKey: key) { candidate in
            var tabResults = candidate.search.activeResults()
            tabResults.isSearching = true
            candidate.search.setResults(
                tabResults,
                forTab: capturedTab,
                source: capturedSource
            )
            candidate.error = nil
        }

        let task = Task { @MainActor in
            do {
                let response = try await importer.searchForCandidate(query)
                applySearchResults(
                    response,
                    importStore: importStore,
                    key: key
                )
            }
            catch is CancellationError {
                logger.debug(
                    "Search cancelled for key: \(key)"
                )
                clearSearching(
                    importStore: importStore,
                    key: key,
                    tab: capturedTab,
                    source: capturedSource,
                    error: nil
                )
            }
            catch {
                logger.error(
                    "Search failed: \(error.localizedDescription)"
                )
                clearSearching(
                    importStore: importStore,
                    key: key,
                    tab: capturedTab,
                    source: capturedSource,
                    error: "Search failed: \(error.displayLine)"
                )
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.searchTask = CancelOnDeinit(task)
        }
    }

    /// The bridge query for the active tab: the general (artist/album),
    /// catalog-number, or barcode field set, all scoped to the active source.
    @MainActor
    private static func searchQuery(
        from search: CandidateSearchState
    ) -> BridgeSearchQuery {
        switch search.activeTab {
        case .general:
            .general(
                artist: search.searchArtist,
                album: search.searchAlbum,
                source: search.activeSource
            )
        case .catalogNumber:
            .catalogNumber(
                catalogNumber: search.searchCatalog,
                source: search.activeSource
            )
        case .barcode:
            .barcode(
                barcode: search.searchBarcode,
                source: search.activeSource
            )
        }
    }

    /// Write a completed search response onto the candidate: replace the active
    /// tab's results and merge in each release's library status.
    @MainActor
    private static func applySearchResults(
        _ response: BridgeCandidateSearchResults,
        importStore: ImportStore,
        key: String
    ) {
        let searchTab: SearchTab =
            switch response.tab {
            case .general: .general
            case .catalogNumber: .catalogNumber
            case .barcode: .barcode
            }
        let resultSource = response.source
        importStore.mutateCandidate(forKey: key) { candidate in
            var tabResults = CandidateSearchState.TabResults()
            tabResults.groups = response.groups.map(ReleaseGroup.init(bridge:))
            tabResults.hasSearched = true
            tabResults.isSearching = false
            candidate.search.setResults(
                tabResults,
                forTab: searchTab,
                source: resultSource
            )
            for status in response.statuses {
                candidate.libraryStatuses[status.releaseId] = status
            }
            candidate.searchTask = nil
        }
    }

    /// Clear the in-flight spinner on the captured tab (cancel or failure). On
    /// failure, `error` is set and the search task is dropped; on cancel both
    /// stay as-is (a fresh search owns the task).
    @MainActor
    private static func clearSearching(
        importStore: ImportStore,
        key: String,
        tab: SearchTab,
        source: BridgeMetadataSource,
        error: String?
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            if let error {
                candidate.error = error
            }
            var tabResults = candidate.search.results(
                forTab: tab,
                source: source
            )
            tabResults.isSearching = false
            candidate.search.setResults(
                tabResults,
                forTab: tab,
                source: source
            )
            if error != nil {
                candidate.searchTask = nil
            }
        }
    }
}

extension ImportSearchFlow {
    // MARK: - Add as Unknown

    /// Project the candidate's audio files into a `ReleaseUserEdit`
    /// shape via the bridge's file-tag preview, seed the editor with
    /// the result, mark the choice as Unknown, and transition to the
    /// confirming mode. Errors fall back to the identifying state with
    /// a banner so the user can retry or pick a search match instead.
    @MainActor
    static func addAsUnknown(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        folderPath: String
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mode = .loadingDetail
            candidate.error = nil
            candidate.identityChoice = .unknown
            // Unknown imports never carry a source release — clear any
            // prior detail and seed so the confirmation page falls back to its
            // detail-less rendering (no remote cover picker, no
            // library-status banner, no track-count mismatch, no
            // Exact/Metadata choice).
            candidate.releaseDetailBridge = nil
            candidate.releaseSeed = nil
            // No source cover exists for Unknown; leave the local
            // artwork picker as the only cover affordance.
            candidate.selectedCover = nil
        }

        let task = Task { @MainActor in
            do {
                let preview = try await importer.previewFileTagsForFolder(
                    folderPath
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.editValues = rawReleaseEditFromUserEdit(
                        edit: preview,
                        trackIdPrefix: "unknown-track"
                    )
                    candidate.mode = .confirming
                    candidate.prefetchTask = nil
                }
            }
            catch is CancellationError {
                logger.debug(
                    "Add as Unknown cancelled for key: \(key)"
                )
            }
            catch {
                logger.error(
                    "Add as Unknown failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.mode = .identifying
                    candidate.identityChoice = nil
                    candidate.error =
                        "Couldn't read file tags: \(error.displayLine)"
                    candidate.prefetchTask = nil
                }
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.prefetchTask = CancelOnDeinit(task)
        }
    }

    // MARK: - Prefetch and confirm

    /// The pressing the user picked to prefetch: the search `result`, the
    /// `identityChoice` made at row-time (carried through to the confirmation
    /// page so commit applies it), and the local track count to reconcile the
    /// fetched detail against.
    struct PrefetchSelection {
        let result: BridgeMetadataResult
        let identityChoice: BridgeIdentityChoice
        let localTrackCount: UInt32?
    }

    @MainActor
    static func prefetchAndConfirm(
        library: Library,
        importStore: ImportStore,
        key: String,
        selection: PrefetchSelection
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mode = .loadingDetail
            candidate.error = nil
            // The choice was made at row-time. Carry it through
            // prefetch into the confirmation page so commit can apply it.
            candidate.identityChoice = selection.identityChoice
        }

        let releaseId = selection.result.releaseId
        let bridgeSource = selection.result.source
        let task = Task { @MainActor in
            do {
                let prefetch = try await library.prefetchRelease(
                    releaseId,
                    bridgeSource,
                    selection.localTrackCount
                )
                let preview = shapeUserEditForChoice(
                    seed: prefetch.seed,
                    choice: selection.identityChoice
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    // The display detail: cover options, track-count mismatch,
                    // library status.
                    candidate.releaseDetailBridge = prefetch.detail
                    // The editor's seed. Kept so flipping the Exact /
                    // Metadata-only choice in the pane re-shapes it without a
                    // re-fetch.
                    candidate.releaseSeed = prefetch.seed
                    // Manual prefetch: the user just picked a different
                    // release, so any prior pick was for a now-stale cover
                    // set — replace it with the new release's default.
                    candidate.selectedCover = prefetch.detail.defaultCover
                    // The seed arrives pre-shaped from bae-core, which projects
                    // it from the release the way the commit worker maps it;
                    // `shapeUserEditForChoice` masks the pressing fields per the
                    // identity claim. `rawReleaseEditFromUserEdit` projects that
                    // wire edit into the raw form the editor binds.
                    candidate.editValues = rawReleaseEditFromUserEdit(
                        edit: preview,
                        trackIdPrefix: "import-track"
                    )
                    candidate.mode = .confirming
                    candidate.prefetchTask = nil
                }
            }
            catch is CancellationError {
                logger.debug(
                    "Prefetch cancelled for key: \(key)"
                )
            }
            catch {
                logger.error(
                    "Prefetch failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.mode = .identifying
                    candidate.error =
                        "Failed to load release details: \(error.displayLine)"
                    candidate.prefetchTask = nil
                }
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.prefetchTask = CancelOnDeinit(task)
        }
    }

    // MARK: - Import status helpers

    @MainActor
    static func isImporting(_ candidate: Candidate) -> Bool {
        guard let status = candidate.importStatus else {
            return false
        }
        switch status {
        case .importing, .complete: return true
        default: return false
        }
    }

}

extension ImportSearchFlow {
    // MARK: - Shared search pane builder

    /// The import-flow services a search pane drives: search/identify on
    /// `importer`, prefetch on `library`, candidate state on `importStore`, and
    /// Discogs availability on `configStore`.
    struct ImportServices {
        let importer: Importer
        let library: Library
        let importStore: ImportStore
        let configStore: ConfigStore
    }

    /// Which candidate a search pane renders, and the selection state it shows:
    /// `selectedReleaseId` is the pressing whose confirm pane is open (so its row
    /// renders selected); `localTrackCount` reconciles a picked release's detail.
    struct SearchPaneInput {
        let candidate: Candidate
        let key: String
        let localTrackCount: UInt32?
        let selectedReleaseId: String?
    }

    /// `onSelect` defaults to the import flow's prefetch + docked-pane path
    /// (picking a pressing opens the confirm pane). Re-identify overrides it
    /// to select the pressing and commit `re_identify_release` from its own
    /// footer, since the release is already in the library — there's no second
    /// editable confirmation page.
    @MainActor
    @ViewBuilder
    static func buildSearchPane(
        services: ImportServices,
        input: SearchPaneInput,
        openSettings: @escaping () -> Void,
        onAddAsUnknown: (() -> Void)?,
        onSelect: ((BridgeMetadataResult) -> Void)? = nil
    ) -> some View {
        let key = input.key
        let importStore = services.importStore
        // Picking a pressing row defaults to claiming it as the exact pressing;
        // the pane's Import-as toggle flips it to Metadata-only afterward.
        let resolvedOnSelect =
            onSelect ?? defaultOnSelect(services: services, input: input)
        let fields = searchFieldBindings(importStore: importStore, input: input)

        ImportSearchPane(
            state: searchPaneState(
                candidate: input.candidate,
                services: services,
                input: input
            ),
            activeTab: fields.activeTab,
            activeSource: fields.activeSource,
            searchArtist: fields.artist,
            searchAlbum: fields.album,
            searchCatalog: fields.catalog,
            searchBarcode: fields.barcode,
            onSearch: {
                dispatchSearch(
                    importer: services.importer,
                    importStore: importStore,
                    key: key
                )
            },
            onOpenSettings: openSettings,
            onSearchManually: setManualSearch(
                importStore: importStore,
                key: key,
                on: true
            ),
            onViewMatches: setManualSearch(
                importStore: importStore,
                key: key,
                on: false
            ),
            onAddAsUnknown: onAddAsUnknown,
            onToggleSignal: { signal in
                services.importer.toggleSignalForCandidate(key, signal)
            },
            onRerun: { services.importer.rerunIdentifyForCandidate(key) },
            onSelect: resolvedOnSelect,
        )
    }

    /// The tab/source/text bindings the pane's form edits, each writing back
    /// through `mutateCandidate` so edits land on the candidate in the store.
    struct SearchFieldBindings {
        let activeTab: Binding<SearchTab>
        let activeSource: Binding<BridgeMetadataSource>
        let artist: Binding<String>
        let album: Binding<String>
        let catalog: Binding<String>
        let barcode: Binding<String>
    }

    @MainActor
    private static func searchFieldBindings(
        importStore: ImportStore,
        input: SearchPaneInput
    ) -> SearchFieldBindings {
        let candidate = input.candidate
        let key = input.key
        func text(
            _ field: WritableKeyPath<CandidateSearchState, String>
        ) -> Binding<String> {
            makeSearchFieldBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: field
            )
        }
        return SearchFieldBindings(
            activeTab: makeActiveTabBinding(
                importStore: importStore,
                key: key,
                candidate: candidate
            ),
            activeSource: makeActiveSourceBinding(
                importStore: importStore,
                key: key,
                candidate: candidate
            ),
            artist: text(\.searchArtist),
            album: text(\.searchAlbum),
            catalog: text(\.searchCatalog),
            barcode: text(\.searchBarcode)
        )
    }

    /// Show (`on: true`) or hide (`on: false`) the manual-search fields by
    /// flipping the candidate's `showManualSearch` flag.
    @MainActor
    private static func setManualSearch(
        importStore: ImportStore,
        key: String,
        on: Bool
    ) -> () -> Void {
        {
            importStore.mutateCandidate(forKey: key) {
                $0.search.showManualSearch = on
            }
        }
    }

    /// The pane's read-only state snapshot from the candidate, plus the
    /// open-confirm selection and Discogs availability the pane renders against.
    @MainActor
    private static func searchPaneState(
        candidate: Candidate,
        services: ImportServices,
        input: SearchPaneInput
    ) -> ImportSearchState {
        let tabResults = candidate.search.activeResults()
        return ImportSearchState(
            identifyState: candidate.identifyState,
            showManualSearch: candidate.search.showManualSearch,
            error: candidate.error,
            searchGroups: tabResults.groups,
            selectedReleaseId: input.selectedReleaseId,
            isSearching: tabResults.isSearching,
            hasSearched: tabResults.hasSearched,
            isImporting: isImporting(candidate),
            libraryStatuses: candidate.libraryStatuses,
            discogsEnabled: services.configStore.config.discogsUsable,
            signals: candidate.signals,
            signalsToolbar: candidate.signalsToolbar,
        )
    }

    /// The default row-pick handler: claim the picked pressing as Exact and run
    /// `prefetchAndConfirm`. Re-identify overrides this with its own `onSelect`.
    @MainActor
    private static func defaultOnSelect(
        services: ImportServices,
        input: SearchPaneInput
    ) -> (BridgeMetadataResult) -> Void {
        { result in
            prefetchAndConfirm(
                library: services.library,
                importStore: services.importStore,
                key: input.key,
                selection: PrefetchSelection(
                    result: result,
                    identityChoice: .exact(
                        releaseId: result.releaseId,
                        source: result.source
                    ),
                    localTrackCount: input.localTrackCount
                )
            )
        }
    }

    // MARK: - Import-as choice (in the pane)

    /// Flip the open pane's Exact / Metadata-only choice and re-seed the
    /// editor from the stored seed. Exact keeps the picked release's pressing
    /// fields; Metadata-only blanks them. Re-shaping is bae-core's job —
    /// `shape_user_edit_for_choice` masks the pressing fields per the choice —
    /// so this re-runs it rather than mutating fields in Swift.
    ///
    /// `seed` and `ref` come from the call site (the toggle only renders for
    /// a source-backed pick, so both are in hand there) — no in-closure lookup
    /// or guard.
    @MainActor
    static func changeChoice(
        importStore: ImportStore,
        key: String,
        seed: BridgeReleaseUserEdit,
        ref: (releaseId: String, source: BridgeMetadataSource),
        wantExact: Bool
    ) {
        let choice = BridgeIdentityChoice.make(
            exact: wantExact,
            releaseId: ref.releaseId,
            source: ref.source
        )
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.identityChoice = choice
            let preview = shapeUserEditForChoice(
                seed: seed,
                choice: choice
            )
            candidate.editValues = rawReleaseEditFromUserEdit(
                edit: preview,
                trackIdPrefix: "import-track"
            )
        }
    }

    // MARK: - Shared confirmation view builder

    /// Two-way binding into the candidate's `editValues` field. Edits
    /// from the embedded edit-metadata form route through here into the
    /// import store; commit reads the live value to build the import
    /// command's `user_edit` overlay.
    @MainActor
    static func makeEditValuesBinding(
        importStore: ImportStore,
        key: String,
        candidate: Candidate
    ) -> Binding<BridgeRawReleaseEdit> {
        // The candidate's editValues was seeded by prefetchAndConfirm
        // before transitioning to .confirming, so the optional is
        // populated by the time this binding is read. Force-unwrap on
        // get keeps the binding non-optional for the form.
        Binding(
            get: {
                guard let values = candidate.editValues else {
                    preconditionFailure(
                        "editValues must be seeded before the confirm binding is read"
                    )
                }
                return values
            },
            set: { newValue in
                importStore.mutateCandidate(forKey: key) {
                    $0.editValues = newValue
                }
            },
        )
    }

    /// The candidate, services, and source-detail-derived display inputs a
    /// confirmation view renders. The detail fields (track-count mismatch,
    /// library status, remote cover art) are discrete rather than a whole
    /// `BridgeReleaseDetail`, so Unknown imports can supply their
    /// file-tag-derived equivalents (no source release id, no remote cover art,
    /// no track-count source to mismatch against).
    struct ConfirmationInputs {
        let importStore: ImportStore
        let key: String
        let uiStore: UiStore
        let trackCountMismatch: Bool
        let expectedTrackCount: UInt32
        let libraryStatus: BridgeLibraryStatus?
        let remoteCoverArts: [BridgeRemoteCover]
        let hasCoverOptions: Bool
        let storageManaged: Binding<Bool>
        let storagePinned: Binding<Bool>
        let localArtwork: [BridgeArtworkFile]
    }

    /// The confirmation view's action callbacks: commit the import, and open the
    /// just-imported release in the library.
    struct ConfirmationCallbacks {
        let onConfirmImport: () -> Void
        let onViewInLibrary: (String) -> Void
    }

    /// Build the confirmation view for a candidate.
    @MainActor
    @ViewBuilder
    static func buildConfirmationView(
        inputs: ConfirmationInputs,
        callbacks: ConfirmationCallbacks,
        @ViewBuilder coverContent: @escaping () -> some View
    ) -> some View {
        let importStore = inputs.importStore
        let key = inputs.key
        let uiStore = inputs.uiStore
        let candidate = importStore.candidate(forKey: key)
        let selectedCover = candidate?.selectedCover
        let importing = candidate.map(isImporting) ?? false

        if let candidate {
            ImportConfirmationView(
                values: makeEditValuesBinding(
                    importStore: importStore,
                    key: key,
                    candidate: candidate
                ),
                storageManaged: inputs.storageManaged,
                storagePinned: inputs.storagePinned,
                trackCountMismatch: inputs.trackCountMismatch,
                expectedTrackCount: inputs.expectedTrackCount,
                libraryStatus: inputs.libraryStatus,
                candidateKey: key,
                importStatus: candidate.importStatus,
                error: candidate.error,
                hasCoverOptions: inputs.hasCoverOptions,
                importing: importing,
                exactness: exactnessChoice(
                    for: candidate,
                    importStore: importStore,
                    key: key
                ),
                onConfirmImport: callbacks.onConfirmImport,
                onViewInLibrary: callbacks.onViewInLibrary,
                onEditCover: {
                    uiStore.presentModal {
                        CoverPickerView(
                            remoteCoverArts: inputs.remoteCoverArts,
                            localArtwork: inputs.localArtwork,
                            selectedCover: selectedCover,
                            onSelect: { selection in
                                importStore.mutateCandidate(forKey: key) {
                                    $0.selectedCover = selection
                                }
                                uiStore.dismissModal()
                            },
                            onDone: { uiStore.dismissModal() },
                        )
                        .frame(width: 600, height: 500)
                    }
                },
                coverContent: coverContent,
            )
        }
    }

    /// The Exact / Metadata-only toggle, or `nil` when it doesn't apply. The
    /// toggle renders only for a source-backed pick (one with a stored seed and
    /// a release ref); Unknown imports have neither and get no toggle.
    /// Unwrapping the seed/ref here means `changeChoice` needs no in-closure
    /// guard.
    @MainActor
    private static func exactnessChoice(
        for candidate: Candidate,
        importStore: ImportStore,
        key: String
    ) -> ImportExactnessChoice? {
        guard let seed = candidate.releaseSeed,
            let choice = candidate.identityChoice,
            let ref = choice.releaseRef
        else {
            return nil
        }
        return ImportExactnessChoice(
            isMetadataOnly: choice.isApproximate,
            onSelect: { wantExact in
                changeChoice(
                    importStore: importStore,
                    key: key,
                    seed: seed,
                    ref: ref,
                    wantExact: wantExact
                )
            }
        )
    }
}
