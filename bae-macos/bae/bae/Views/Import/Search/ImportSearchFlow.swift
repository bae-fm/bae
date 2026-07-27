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

    /// Two-way binding into the candidate's `editValues`. The mapping pane's
    /// release fields and slot rows route through here into the import store;
    /// commit reads the live value to build the import command's `user_edit`
    /// overlay.
    @MainActor
    static func makeEditValuesBinding(
        importStore: ImportStore,
        key: String,
        candidate: Candidate
    ) -> Binding<BridgeRawReleaseEdit> {
        // A pick, or "Add as Unknown", seeds `editValues` before anything binds
        // it, so the optional is populated by the time this is read. The
        // precondition keeps the binding non-optional for the fields.
        Binding(
            get: {
                guard let values = candidate.editValues else {
                    preconditionFailure(
                        "editValues must be seeded before the editor binding is read"
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
                    error: error.displayLine.map { "Search failed: \($0)" }
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
