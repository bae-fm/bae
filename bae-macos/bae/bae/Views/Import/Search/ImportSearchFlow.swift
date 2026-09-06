import BaeKit
import SwiftUI

enum ImportSearchFlow {
    // MARK: - Search dispatch

    /// Submit the form's query. Fire-and-forget: every configured provider is
    /// asked at once and each answer lands on the candidate's runtime, which
    /// the pane draws — so nothing here waits for a result or holds one.
    @MainActor
    static func startSearch(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        form: CandidateSearchState
    ) {
        importStore.clearPaneError(forKey: key)
        importer.startCandidateSearch(key, searchQuery(from: form))
    }

    @MainActor
    static func searchRelease(
        services: ImportServices,
        key: String,
        artist: String?,
        title: String,
        source: BridgeMetadataSource
    ) {
        let form = CandidateSearchState(
            searchArtist: artist ?? "",
            searchAlbum: title
        )
        services.importStore.commitSearchForm(form, forKey: key)
        services.importStore.clearPaneError(forKey: key)
        services.importer.startSourceCandidateSearch(
            key,
            searchQuery(from: form),
            source: source
        )
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
