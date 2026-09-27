import BaeKit
import SwiftUI

enum ImportSearchFlow {
    // MARK: - Search dispatch

    /// Submit the form's query: every configured provider is asked at once and
    /// each answer lands on the candidate's runtime, which the pane draws — so
    /// nothing here waits for a result or holds one. Core clears the failure
    /// the pane states as the search starts.
    @MainActor
    static func startSearch(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        form: CandidateSearchState
    ) {
        let query = searchQuery(from: form)
        Task { @MainActor in
            do { try await importer.startCandidateSearch(key, query) }
            catch { importStore.reportFailure(error) }
        }
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
