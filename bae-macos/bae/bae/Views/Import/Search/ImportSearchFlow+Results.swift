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
        refreshLibraryStatusSubscriptions(
            importer: importer,
            importStore: importStore,
            key: key
        )
    }

    @MainActor
    private static func refreshLibraryStatusSubscriptions(
        importer: Importer,
        importStore: ImportStore,
        key: String
    ) {
        guard let candidate = importStore.candidate(forKey: key) else { return }
        let desired = candidate.search.libraryStatusSubscriptionKeys()

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.libraryStatusSubscriptions =
                candidate.libraryStatusSubscriptions.filter {
                    desired.contains($0.key)
                }
        }

        for statusKey in desired {
            guard
                importStore.candidate(forKey: key)?
                    .libraryStatusSubscriptions[statusKey] == nil
            else { continue }
            let subscription = importer.subscribeReleaseLibraryStatus(
                source: statusKey.source,
                releaseId: statusKey.releaseId,
                sourceGroupId: statusKey.sourceGroupId,
                onValue: { [weak importStore] status in
                    importStore?
                        .mutateCandidate(forKey: key) {
                            $0.libraryStatuses[status.releaseId] = status
                        }
                },
                onError: { [weak importStore] error in
                    guard let line = error.displayLine else { return }
                    importStore?
                        .mutateCandidate(forKey: key) {
                            $0.error = line
                        }
                }
            )
            importStore.mutateCandidate(forKey: key) {
                $0.libraryStatusSubscriptions[statusKey] =
                    CancelOnDeinit(subscription)
            }
        }
    }
}
