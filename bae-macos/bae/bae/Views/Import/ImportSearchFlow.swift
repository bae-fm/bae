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
    ) -> Binding<MetadataSource> {
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

        let query: BridgeSearchQuery =
            switch capturedTab {
            case .general:
                .general(
                    artist: snapshot.search.searchArtist,
                    album: snapshot.search.searchAlbum,
                    source: capturedSource.bridge
                )
            case .catalogNumber:
                .catalogNumber(
                    catalogNumber: snapshot.search.searchCatalog,
                    source: capturedSource.bridge
                )
            case .barcode:
                .barcode(
                    barcode: snapshot.search.searchBarcode,
                    source: capturedSource.bridge
                )
            }

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
                let searchTab: SearchTab =
                    switch response.tab {
                    case .general: .general
                    case .catalogNumber: .catalogNumber
                    case .barcode: .barcode
                    }
                let resultSource = MetadataSource(bridge: response.source)
                importStore.mutateCandidate(forKey: key) { candidate in
                    var tabResults = CandidateSearchState.TabResults()
                    tabResults.groups = response.groups.map(
                        ReleaseGroup.init(bridge:)
                    )
                    tabResults.hasSearched = true
                    tabResults.isSearching = false
                    candidate.search.setResults(
                        tabResults,
                        forTab: searchTab,
                        source: resultSource
                    )
                    for status in response.statuses {
                        candidate.libraryStatuses[status.releaseId] =
                            LibraryStatus(bridge: status)
                    }
                    candidate.searchTask = nil
                }
            }
            catch is CancellationError {
                logger.debug(
                    "Search cancelled for key: \(key, privacy: .public)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    var tabResults = candidate.search.results(
                        forTab: capturedTab,
                        source: capturedSource
                    )
                    tabResults.isSearching = false
                    candidate.search.setResults(
                        tabResults,
                        forTab: capturedTab,
                        source: capturedSource
                    )
                }
            }
            catch {
                logger.error(
                    "Search failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.error =
                        "Search failed: \(error.localizedDescription)"
                    var tabResults = candidate.search.results(
                        forTab: capturedTab,
                        source: capturedSource
                    )
                    tabResults.isSearching = false
                    candidate.search.setResults(
                        tabResults,
                        forTab: capturedTab,
                        source: capturedSource
                    )
                    candidate.searchTask = nil
                }
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.searchTask = CancelOnDeinit(task)
        }
    }

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
            // prior detail so the confirmation page falls back to its
            // detail-less rendering (no remote cover picker, no
            // library-status banner, no track-count mismatch, no
            // Exact/Metadata choice).
            candidate.releaseDetailBridge = nil
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
                    "Add as Unknown cancelled for key: \(key, privacy: .public)"
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
                        "Couldn't read file tags: \(error.localizedDescription)"
                    candidate.prefetchTask = nil
                }
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.prefetchTask = CancelOnDeinit(task)
        }
    }

    // MARK: - Prefetch and confirm

    @MainActor
    static func prefetchAndConfirm(
        library: Library,
        importStore: ImportStore,
        key: String,
        result: MetadataResult,
        identityChoice: IdentityChoice,
        localTrackCount: UInt32?
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mode = .loadingDetail
            candidate.error = nil
            // The choice was made at row-time. Carry it through
            // prefetch into the confirmation page so commit can apply it.
            candidate.identityChoice = identityChoice
        }

        let releaseId = result.releaseId
        let bridgeSource = result.source.bridge
        let task = Task { @MainActor in
            do {
                let bridgeDetail = try await library.prefetchRelease(
                    releaseId,
                    bridgeSource,
                    localTrackCount
                )
                let preview = shapeUserEditFromReleaseDetail(
                    detail: bridgeDetail,
                    choice: identityChoice.bridge
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    // Keep the full source detail: flipping the Exact /
                    // Metadata-only choice in the pane re-shapes the editor
                    // from it without a re-fetch.
                    candidate.releaseDetailBridge = bridgeDetail
                    // Manual prefetch: the user just picked a different
                    // release, so any prior pick was for a now-stale cover
                    // set — replace it with the new release's default.
                    candidate.selectedCover = bridgeDetail.defaultCover
                    // Editor seed comes pre-shaped from bae-core (the
                    // Exact-vs-Approximate/Unknown pressing-field
                    // masking and per-track artist-override logic live
                    // there, not in Swift). `rawReleaseEditFromUserEdit`
                    // projects that wire edit into the raw form the
                    // editor binds.
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
                    "Prefetch cancelled for key: \(key, privacy: .public)"
                )
            }
            catch {
                logger.error(
                    "Prefetch failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.mode = .identifying
                    candidate.error =
                        "Failed to load release details: \(error.localizedDescription)"
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

    // MARK: - Shared search pane builder

    /// `onSelect` defaults to the import flow's prefetch + docked-pane path
    /// (picking a pressing opens the confirm pane). Re-identify overrides it
    /// to select the pressing and commit `re_identify_release` from its own
    /// footer, since the release is already in the library — there's no second
    /// editable confirmation page.
    @MainActor
    @ViewBuilder
    static func buildSearchPane(
        importer: Importer,
        library: Library,
        importStore: ImportStore,
        configStore: ConfigStore,
        key: String,
        candidate: Candidate,
        localTrackCount: UInt32?,
        openSettings: @escaping () -> Void,
        /// Release id of the pressing whose confirm pane is open, so its row
        /// renders selected.
        selectedReleaseId: String?,
        onAddAsUnknown: (() -> Void)?,
        onSelect: ((MetadataResult) -> Void)? = nil,
    ) -> some View {
        let tabResults = candidate.search.activeResults()
        // Picking a pressing row defaults to claiming it as the exact pressing;
        // the pane's Import-as toggle flips it to Metadata-only afterward.
        let resolvedOnSelect: (MetadataResult) -> Void =
            onSelect
            ?? { result in
                prefetchAndConfirm(
                    library: library,
                    importStore: importStore,
                    key: key,
                    result: result,
                    identityChoice: .exact(
                        releaseId: result.releaseId,
                        source: result.source
                    ),
                    localTrackCount: localTrackCount
                )
            }

        ImportSearchPane(
            state: ImportSearchState(
                identifyState: candidate.identifyState,
                showManualSearch: candidate.search.showManualSearch,
                error: candidate.error,
                searchGroups: tabResults.groups,
                selectedReleaseId: selectedReleaseId,
                isSearching: tabResults.isSearching,
                hasSearched: tabResults.hasSearched,
                isImporting: isImporting(candidate),
                libraryStatuses: candidate.libraryStatuses,
                discogsEnabled: configStore.config.discogsUsable,
                signals: candidate.signals,
                signalsToolbar: candidate.signalsToolbar,
            ),
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
            searchArtist: makeSearchFieldBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: \.searchArtist
            ),
            searchAlbum: makeSearchFieldBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: \.searchAlbum
            ),
            searchCatalog: makeSearchFieldBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: \.searchCatalog
            ),
            searchBarcode: makeSearchFieldBinding(
                importStore: importStore,
                key: key,
                candidate: candidate,
                field: \.searchBarcode
            ),
            onSearch: {
                dispatchSearch(
                    importer: importer,
                    importStore: importStore,
                    key: key
                )
            },
            onOpenSettings: openSettings,
            onSearchManually: {
                importStore.mutateCandidate(forKey: key) {
                    $0.search.showManualSearch = true
                }
            },
            onViewMatches: {
                importStore.mutateCandidate(forKey: key) {
                    $0.search.showManualSearch = false
                }
            },
            onAddAsUnknown: onAddAsUnknown,
            onToggleSignal: { signal in
                importer.toggleSignalForCandidate(key, signal.bridge)
            },
            onRerun: {
                importer.rerunIdentifyForCandidate(key)
            },
            onSelect: resolvedOnSelect,
        )
    }

    // MARK: - Import-as choice (in the pane)

    /// Flip the open pane's Exact / Metadata-only choice and re-seed the
    /// editor from the stored source detail. Exact seeds the pressing fields
    /// from the picked release; Metadata-only blanks them. Re-shaping is
    /// bae-core's job — `shape_user_edit_from_search_detail` masks the pressing
    /// fields per the choice — so this re-runs it rather than mutating fields
    /// in Swift.
    ///
    /// `detail` and `ref` come from the call site (the toggle only renders for
    /// a source-backed pick, so both are in hand there) — no in-closure lookup
    /// or guard.
    @MainActor
    static func changeChoice(
        importStore: ImportStore,
        key: String,
        detail: BridgeReleaseDetail,
        ref: (releaseId: String, source: MetadataSource),
        wantExact: Bool
    ) {
        let choice = IdentityChoice.make(
            exact: wantExact,
            releaseId: ref.releaseId,
            source: ref.source
        )
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.identityChoice = choice
            let preview = shapeUserEditFromReleaseDetail(
                detail: detail,
                choice: choice.bridge
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
            get: { candidate.editValues! },
            set: { newValue in
                importStore.mutateCandidate(forKey: key) {
                    $0.editValues = newValue
                }
            },
        )
    }

    /// Build the confirmation view for a candidate. Source-detail
    /// fields (track-count mismatch, library status, remote cover art)
    /// are passed as discrete inputs rather than a whole
    /// `ImportReleaseDetail`, so Unknown imports can supply their
    /// file-tag-derived equivalents (no source release id, no remote
    /// cover art, no track-count source to mismatch against).
    @MainActor
    @ViewBuilder
    static func buildConfirmationView(
        importStore: ImportStore,
        key: String,
        trackCountMismatch: Bool,
        expectedTrackCount: UInt32,
        libraryStatus: LibraryStatus?,
        remoteCoverArts: [BridgeRemoteCover],
        hasCoverOptions: Bool,
        storageManaged: Binding<Bool>,
        storagePinned: Binding<Bool>,
        importDisabled: Bool,
        localArtwork: [ArtworkFile],
        uiStore: UiStore,
        onConfirmImport: @escaping () -> Void,
        onViewInLibrary: @escaping (String) -> Void,
        @ViewBuilder coverContent: @escaping () -> some View,
        @ViewBuilder actionExtra: @escaping () -> some View,
    ) -> some View {
        let candidate = importStore.candidate(forKey: key)
        let selectedCover = candidate?.selectedCover
        let importing = candidate.map(isImporting) ?? false

        if let candidate {
            // The Exact / Metadata-only toggle applies only to a source-backed
            // pick; Unknown imports have no detail or release ref and get no
            // toggle. Unwrapping here means `changeChoice` needs no in-closure
            // guard.
            let exactness: ImportExactnessChoice? = {
                guard let detail = candidate.releaseDetailBridge,
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
                            detail: detail,
                            ref: ref,
                            wantExact: wantExact
                        )
                    }
                )
            }()
            ImportConfirmationView(
                values: makeEditValuesBinding(
                    importStore: importStore,
                    key: key,
                    candidate: candidate
                ),
                storageManaged: storageManaged,
                storagePinned: storagePinned,
                importDisabled: importDisabled,
                trackCountMismatch: trackCountMismatch,
                expectedTrackCount: expectedTrackCount,
                libraryStatus: libraryStatus,
                importStatus: candidate.importStatus,
                error: candidate.error,
                hasCoverOptions: hasCoverOptions,
                importing: importing,
                exactness: exactness,
                onConfirmImport: onConfirmImport,
                onViewInLibrary: onViewInLibrary,
                onEditCover: {
                    uiStore.presentModal {
                        CoverPickerView(
                            remoteCoverArts: remoteCoverArts,
                            localArtwork: localArtwork,
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
                actionExtra: actionExtra,
            )
        }
    }
}
