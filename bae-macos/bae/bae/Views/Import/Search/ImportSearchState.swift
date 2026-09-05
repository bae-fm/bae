import BaeKit

/// Everything `ImportSearchPane` renders from — the identify verdict, the
/// typed search, and the surrounding flags. The editable form fields (search
/// bindings) and the actions stay separate; this is the read-only display
/// state.
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
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let isImporting: Bool
    /// Whether core is still committing the verdict the pane shows: fetching
    /// the sole pressing's details, then storing the answer. The verdict is
    /// final; what is pending is the pick and the row.
    let isFinalizing: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let signals: Signals?
    /// The signals core extracted, as the pre-shaped badge list the Adjust
    /// popover renders. Empty until the first identify transition, and for a
    /// verdict resumed from the store — whose raw signals were never stored.
    let signalsToolbar: BridgeSignalsToolbar

    /// The album cards identification is offering. A run still going offers
    /// what has landed so far; a failed run still carries whatever the
    /// surviving source found.
    var identifiedGroups: [ReleaseGroup] {
        switch identifyState {
        case .found(let groups, _, _, _): groups
        case .failed(_, let groups, _, _): groups
        case .triangulating(_, let groups, _, _): groups
        case .idle, .notFoundAnywhere, .manualOnly: []
        }
    }

    /// Which signals produced each offered pressing, keyed by release id.
    var identifiedProvenance: [String: BridgeResultProvenance] {
        switch identifyState {
        case .found(_, _, _, let provenance): provenance
        case .failed(_, _, _, let provenance): provenance
        case .triangulating(_, _, _, let provenance): provenance
        case .idle, .notFoundAnywhere, .manualOnly: [:]
        }
    }

    /// The automatic lookups that failed, each naming what it was and why.
    var identifyFailures: [BridgeIdentifyFailure] {
        guard case .failed(let failures, _, _, _) = identifyState else {
            return []
        }
        return failures
    }
}
