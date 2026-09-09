import BaeKit

/// Everything `ImportSearchPane` renders from — the identify verdict and its
/// run, the typed search, and the surrounding flags. The editable form fields
/// (search bindings) and the actions stay separate; this is the read-only
/// display state.
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
    /// Where each of the candidate's files is on disk, by the
    /// candidate-relative path a signal names it by — what a source chip
    /// crops its thumbnail out of. Empty for a re-identify session, whose
    /// images are stored blobs rather than files.
    let filePaths: [String: String]

    /// The run as its ledger, while there is one to lay out.
    var run: BridgeIdentifyRun? {
        identifyState.run
    }

    /// The album cards identification is offering. A run still going offers
    /// what has landed so far; a failed run still carries whatever the
    /// surviving source found.
    var identifiedGroups: [ReleaseGroup] {
        switch identifyState {
        case .found(_, let groups, _, _, _, _, _): groups
        case .failed(_, _, let groups, _, _, _, _): groups
        case .triangulating(_, let groups, _, _, _): groups
        case .idle, .notFoundAnywhere, .manualOnly: []
        }
    }

    /// The releases the signals' agreement left out of the offered ones —
    /// what the AUTOMATIC section offers behind its disclosure.
    var narrowedOut: NarrowedOut {
        identifyState.narrowedOut
    }

    /// What the candidate's own text agrees with about each offered pressing,
    /// keyed by release id — the row badges, and what ordered the rows.
    var identifiedAgreements: [String: BridgeAgreements] {
        switch identifyState {
        case .found(_, _, _, _, let agreements, _, _): agreements
        case .failed(_, _, _, _, let agreements, _, _): agreements
        case .triangulating(_, _, _, let agreements, _): agreements
        case .idle, .notFoundAnywhere, .manualOnly: [:]
        }
    }

    /// The catalog numbers the folder states about the offered releases — the
    /// chips in the ledger's Catalog # row, each counting until it is struck
    /// out.
    var catalogAgreements: [BridgeCatalogAgreement] {
        identifyState.catalogAgreements
    }

    /// The automatic lookups that failed, each naming what it was and why.
    var identifyFailures: [BridgeIdentifyFailure] {
        guard
            case .failed(_, let failures, _, _, _, _, _) = identifyState
        else {
            return []
        }
        return failures
    }

    /// The pressing core is picking on its own: a sole match selects itself,
    /// and its row holds the spinner while its details fetch and the answer
    /// saves. `nil` when nothing is finalizing, or when the verdict left
    /// several to choose from — those wait on a person.
    var finalizingPressing: Pressing? {
        guard isFinalizing else { return nil }
        let pressings = identifiedGroups.flatMap(\.pressings)
        guard pressings.count == 1 else { return nil }
        return pressings[0]
    }
}
