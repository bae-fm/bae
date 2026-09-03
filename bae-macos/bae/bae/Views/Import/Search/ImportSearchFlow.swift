import BaeKit
import SwiftUI

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

    /// Submit the form's query. Fire-and-forget: every configured provider is
    /// asked at once and each answer lands on the candidate's runtime, which
    /// the pane draws — so nothing here waits for a result or holds one.
    @MainActor
    static func startSearch(
        importer: Importer,
        importStore: ImportStore,
        key: String
    ) {
        guard let snapshot = importStore.candidate(forKey: key) else {
            return
        }
        importStore.mutateCandidate(forKey: key) { $0.error = nil }
        importer.startCandidateSearch(key, searchQuery(from: snapshot.search))
    }

    /// The bridge query for the active tab: the general (artist/album),
    /// catalog-number, or barcode field set.
    @MainActor
    private static func searchQuery(
        from search: CandidateSearchState
    ) -> BridgeSearchQuery {
        switch search.activeTab {
        case .general:
            .general(artist: search.searchArtist, album: search.searchAlbum)
        case .catalogNumber:
            .catalogNumber(catalogNumber: search.searchCatalog)
        case .barcode:
            .barcode(barcode: search.searchBarcode)
        }
    }
}
