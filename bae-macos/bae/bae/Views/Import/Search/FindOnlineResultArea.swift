import BaeKit

/// What the AUTOMATIC section shows under its ledger — or instead of one,
/// when there is nothing to lay out.
///
/// Choosing between them is a switch over one pre-shaped value, so it is
/// stated once here and rendered from.
enum FindOnlineResultArea: Equatable {
    /// A lookup is under way: the ledger, with the matches landed so far
    /// beneath it.
    case identifying
    /// Identification has matches to offer.
    case groups
    /// Identification ran and neither source knew the folder's signals.
    case nothingFound
    /// The folder carries nothing to look up and nothing to offer.
    case noSignals
    /// The folder carries nothing to look up on its own, but catalog numbers
    /// a person can activate: the ledger's tiles, and nothing beneath them.
    case awaitingCatalog
    /// Every lookup that ran failed, so there is nothing but the reasons.
    case failureLines
    /// No lookup has been asked for yet.
    case notStarted

    init(identifyState: IdentifyState) {
        switch identifyState {
        case .idle:
            self = .notStarted
        case .triangulating:
            self = .identifying
        case .found(_, let groups, _, _, _, _, _):
            self = groups.isEmpty ? .nothingFound : .groups
        case .notFoundAnywhere:
            self = .nothingFound
        case .manualOnly(_, let run):
            self = run == nil ? .noSignals : .awaitingCatalog
        case .failed(_, _, let groups, _, _, _, _):
            // One source failing leaves the other's matches standing: show
            // them, with the failure named under the list.
            self = groups.isEmpty ? .failureLines : .groups
        }
    }
}
