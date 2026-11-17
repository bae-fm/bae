import BaeKit

/// Everything `MacFindOnline` renders from — the identify verdict and its
/// run, the typed search, and the surrounding flags. The editable form fields
/// (search bindings) and the actions stay separate; this is the read-only
/// display state.
struct ImportSearchState {
    /// What identification shows for the candidate, as core's runtime read
    /// says; `nil` until that read answers.
    let identifyState: IdentifyState?
    let error: String?
    /// The typed search submitted for this candidate, as its providers land.
    /// `nil` before one is submitted and after it is cleared.
    let search: BridgeCandidateSearch?
    /// The release the person picked, while core reads it in full from the
    /// network. Its row shows which lookup core is waiting on.
    let loadingPick: LoadingPick?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let isImporting: Bool
    let signals: Signals?
    /// Why the folder waits on the person, while it does: the page opens with
    /// the sentence saying so. `nil` otherwise, and for a library release
    /// being identified again, which is not on Found.
    let needsYou: BridgeNeedsYouReason?

    /// The run as its ledger, while there is one to lay out.
    var run: BridgeIdentifyRun? {
        identifyState?.run
    }

    /// The album cards identification is offering, then the cards all of whose
    /// rows agreement set aside. A run still going offers what has landed so
    /// far; a failed run still carries whatever the surviving source found.
    var identifiedGroups: [ReleaseGroup] {
        switch identifyState {
        case .found(_, let groups, _, _, _, _, _): groups
        case .failed(_, _, let groups, _, _, _): groups
        case .triangulating(_, let groups, _, _): groups
        case .idle, .queued, .notFoundAnywhere, .manualOnly, .error, nil: []
        }
    }

    /// How many rows the signals' agreement left out of the offered ones —
    /// what the AUTOMATIC section's disclosure counts.
    var narrowedOutCount: UInt32 {
        identifyState?.narrowedOutCount ?? 0
    }

    /// What the candidate's own text agrees with about each offered pressing,
    /// keyed by release id — the row badges, and what ordered the rows.
    var identifiedAgreements: [String: BridgeAgreements] {
        switch identifyState {
        case .found(_, _, _, let agreements, _, _, _): agreements
        case .failed(_, _, _, let agreements, _, _): agreements
        case .triangulating(_, _, let agreements, _): agreements
        case .idle, .queued, .notFoundAnywhere, .manualOnly, .error, nil: [:]
        }
    }

    /// Whether the offered rows are several pressings of one album, which
    /// the folder can be linked to with its pressing unknown.
    var offersSharedAlbum: Bool {
        identifyState?.offersSharedAlbum ?? false
    }

    /// The check against the folder the found release failed: why nothing
    /// was picked.
    var folderCheck: BridgeFolderCheck? {
        identifyState?.folderCheck
    }

    /// The automatic lookups that failed where no cell of the band shows
    /// them, each said in a line of its own. Core chooses which ones, and
    /// sends `nil` when a cell shows every failure or the run did not fail.
    var failureLines: BridgeFailureLines? {
        guard
            case .failed(_, let failureLines, _, _, _, _) = identifyState
        else { return nil }
        return failureLines
    }
}
