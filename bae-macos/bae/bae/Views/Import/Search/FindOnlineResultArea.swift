import BaeKit

/// Which of the pane's result areas is showing.
///
/// One page shows one result area at a time, and a typed search takes it over
/// while it exists — that is what makes Clear a restore rather than a re-run.
/// Choosing between them is a switch over two pre-shaped values, so it is
/// stated once here and rendered from.
enum FindOnlineResultArea: Equatable {
    /// A lookup is under way: the skeleton rows hold the space its results
    /// will take.
    case identifying
    /// Identification has matches to offer.
    case groups
    /// Identification ran and neither source knew the folder's signals.
    case nothingFound
    /// The folder carries nothing to look up.
    case noSignals
    /// Every lookup that ran failed, so there is nothing but the reasons.
    case failureLines
    /// No lookup has been asked for yet.
    case notStarted
    /// A typed search has been submitted; it owns the area until it is
    /// cleared.
    case searchRun

    init(identifyState: IdentifyState, hasSearch: Bool) {
        if hasSearch {
            self = .searchRun
            return
        }
        switch identifyState {
        case .idle:
            self = .notStarted
        case .triangulating:
            self = .identifying
        case .found(let groups, _, _, _):
            self = groups.isEmpty ? .nothingFound : .groups
        case .notFoundAnywhere:
            self = .nothingFound
        case .manualOnly:
            self = .noSignals
        case .failed(_, let groups, _, _):
            // One source failing leaves the other's matches standing: show
            // them, with the failure named under the list.
            self = groups.isEmpty ? .failureLines : .groups
        }
    }
}
