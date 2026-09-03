import BaeKit

/// Everything `ImportSearchPane` renders from — the identify/results state and
/// the surrounding flags. The editable form fields (search bindings), the mode
/// and the actions stay separate; this is the read-only display state.
struct ImportSearchState {
    let identifyState: IdentifyState
    let error: String?
    /// The typed search submitted for this candidate, as its providers land.
    /// `nil` before one is submitted and after it is cleared.
    let search: BridgeCandidateSearch?
    /// Release id of the pressing whose confirm pane is open, so its row renders
    /// selected.
    let selectedReleaseId: String?
    /// Release id whose fetched candidate detail has not landed yet. The
    /// matching result row swaps its chevron for the existing spinner.
    let loadingReleaseId: String?
    let isImporting: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let discogsEnabled: Bool
    let signals: Signals?
    /// The interactive signals toolbar — the pre-shaped badge list. Empty until
    /// the first identify transition; the toolbar is hidden until then.
    let signalsToolbar: BridgeSignalsToolbar

    /// The search's album cards, or nothing when no search is running.
    var searchGroups: [ReleaseGroup] {
        search?.groups.map(ReleaseGroup.init(bridge:)) ?? []
    }

    /// Whether any provider is still answering the search. Core settles that,
    /// so this reads its answer rather than folding the two source states.
    var isSearching: Bool {
        guard let search else { return false }
        return !search.settled
    }

    /// The providers that did not answer, each with its own reason.
    var searchFailures:
        [(source: BridgeMetadataSource, failure: BridgeLookupFailure)]
    {
        guard let search else { return [] }
        return [
            (BridgeMetadataSource.musicBrainz, search.musicbrainz),
            (BridgeMetadataSource.discogs, search.discogs),
        ]
        .compactMap { source, state in
            guard case .failed(let failure) = state else { return nil }
            return (source, failure)
        }
    }
}
