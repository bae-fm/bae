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
    static func makeSearchSourceBinding(
        importStore: ImportStore,
        key: String,
        candidate: Candidate,
        field: WritableKeyPath<CandidateSearchState, Bool>
    ) -> Binding<Bool> {
        Binding(
            get: { candidate.search[keyPath: field] },
            set: { newValue in
                importStore.mutateCandidate(forKey: key) {
                    $0.search[keyPath: field] = newValue
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
        key: String,
        discogsAvailable: Bool
    ) {
        guard let snapshot = importStore.candidate(forKey: key) else {
            return
        }
        let slot = snapshot.search.activeSlot(
            discogsAvailable: discogsAvailable
        )
        guard let bridgeSources = slot.sources.bridgeSources else {
            return
        }
        let query = searchQuery(
            from: snapshot.search,
            sources: bridgeSources
        )

        markSearching(importStore: importStore, key: key, slot: slot)

        let task = Task { @MainActor in
            do {
                let response = try await importer.searchForCandidate(query)
                applySearchResults(
                    response,
                    importer: importer,
                    importStore: importStore,
                    key: key,
                    slot: slot
                )
            }
            catch is CancellationError {
                logger.debug(
                    "Search cancelled for key: \(key)"
                )
                clearSearching(
                    importStore: importStore,
                    key: key,
                    slot: slot,
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
                    slot: slot,
                    error: error.displayLine.map {
                        String(localized: "Search failed: \($0)")
                    }
                )
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.searchTask = CancelOnDeinit(task)
        }
    }

    @MainActor
    private static func markSearching(
        importStore: ImportStore,
        key: String,
        slot: CandidateSearchSlot
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            var tabResults = candidate.search.results(for: slot)
            tabResults.isSearching = true
            candidate.search.setResults(tabResults, for: slot)
            candidate.error = nil
        }
    }

    /// The bridge query for the active tab: the general (artist/album),
    /// catalog-number, or barcode field set, scoped to the selected providers.
    @MainActor
    private static func searchQuery(
        from search: CandidateSearchState,
        sources: BridgeSearchSources
    ) -> BridgeSearchQuery {
        switch search.activeTab {
        case .general:
            .general(
                artist: search.searchArtist,
                album: search.searchAlbum,
                sources: sources
            )
        case .catalogNumber:
            .catalogNumber(
                catalogNumber: search.searchCatalog,
                sources: sources
            )
        case .barcode:
            .barcode(
                barcode: search.searchBarcode,
                sources: sources
            )
        }
    }

    /// Clear the in-flight spinner on the captured tab (cancel or failure). On
    /// failure, `error` is set and the search task is dropped; on cancel both
    /// stay as-is (a fresh search owns the task).
    @MainActor
    private static func clearSearching(
        importStore: ImportStore,
        key: String,
        slot: CandidateSearchSlot,
        error: String?
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            if let error {
                candidate.error = error
            }
            var tabResults = candidate.search.results(for: slot)
            tabResults.isSearching = false
            candidate.search.setResults(tabResults, for: slot)
            if error != nil {
                candidate.searchTask = nil
            }
        }
    }
}
