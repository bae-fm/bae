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
    /// Whether core is reading in full the release the verdict the pane shows
    /// picked unattended, before it stores the answer.
    let isReadingPick: Bool
    let libraryStatuses: [String: BridgeLibraryStatus]
    let signals: Signals?
    /// Why the folder waits on the person, while it does: the page opens with
    /// the sentence saying so. `nil` otherwise, and for a library release
    /// being identified again, which is not on Found.
    let needsYou: BridgeNeedsYouReason?

    /// The run as its ledger, while there is one to lay out.
    var run: BridgeIdentifyRun? {
        identifyState.run
    }

    /// The album cards identification is offering, then the cards all of whose
    /// rows agreement set aside. A run still going offers what has landed so
    /// far; a failed run still carries whatever the surviving source found.
    var identifiedGroups: [ReleaseGroup] {
        switch identifyState {
        case .found(_, let groups, _, _, _, _, _, _): groups
        case .failed(_, _, let groups, _, _, _): groups
        case .triangulating(_, let groups, _, _): groups
        case .idle, .queued, .notFoundAnywhere, .manualOnly, .error: []
        }
    }

    /// How many rows the signals' agreement left out of the offered ones —
    /// what the AUTOMATIC section's disclosure counts.
    var narrowedOutCount: UInt32 {
        identifyState.narrowedOutCount
    }

    /// What the candidate's own text agrees with about each offered pressing,
    /// keyed by release id — the row badges, and what ordered the rows.
    var identifiedAgreements: [String: BridgeAgreements] {
        switch identifyState {
        case .found(_, _, _, let agreements, _, _, _, _): agreements
        case .failed(_, _, _, let agreements, _, _): agreements
        case .triangulating(_, _, let agreements, _): agreements
        case .idle, .queued, .notFoundAnywhere, .manualOnly, .error: [:]
        }
    }

    /// Whether the offered rows are several pressings of one album, which
    /// the folder can be linked to with its pressing unknown.
    var offersSharedAlbum: Bool {
        identifyState.offersSharedAlbum
    }

    /// The check against the folder the found release failed: why nothing
    /// was picked.
    var folderCheck: BridgeFolderCheck? {
        identifyState.folderCheck
    }

    /// The automatic lookups that failed where no cell of the band shows
    /// them, each said in a line of its own — which ones is core's answer.
    var failureLines: [BridgeIdentifyFailure] {
        guard
            case .failed(_, let failureLines, _, _, _, _) = identifyState
        else {
            return []
        }
        return failureLines
    }

    /// The pressing core picked on its own, whose row holds the spinner while
    /// it is read in full and the answer saves: the one pressing the verdict
    /// offers. `nil` while no pick is being read.
    var pickBeingRead: Pressing? {
        guard isReadingPick else { return nil }
        return identifiedGroups.flatMap(\.pressings).first
    }
}
