import BaeKit

extension ImportSearchFlow {
    /// Write a completed search response onto the candidate: replace the active
    /// tab's results and merge in each release's library status.
    @MainActor
    static func applySearchResults(
        _ response: BridgeCandidateSearchResults,
        importer: Importer,
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
            tabResults.libraryStatusSubscriptionKeys = Set(
                response.groups.flatMap { group in
                    group.pressings.map { pressing in
                        ReleaseLibraryStatusSubscriptionKey(
                            source: pressing.source,
                            releaseId: pressing.releaseId,
                            sourceGroupId: group.sourceGroupId
                        )
                    }
                }
            )
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
        importStore.refreshLibraryStatusSubscriptions(
            importer: importer,
            key: key
        )
    }
}
