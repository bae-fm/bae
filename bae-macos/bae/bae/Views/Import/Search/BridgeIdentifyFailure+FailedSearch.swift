import BaeKit

/// One lookup a provider was asked for: which step of identification, at
/// which source. A failure names one; the same source may well have answered
/// the other steps, and those matches are on the list.
struct FailedSearch: Hashable {
    let source: BridgeCatalog
    let step: Step

    /// The steps a provider answers. Three of them are the identifiers the
    /// badge row names; the title search is the run's own last step, which has
    /// no badge because it is not a value the folder carries.
    enum Step: Hashable {
        case signal(BridgeSignalKind)
        case titleSearch
    }
}

extension BridgeIdentifyFailure {
    /// The lookup this failure names, for the line saying its results are
    /// missing from the list. `nil` for the steps no provider owns. The
    /// disc-ID endpoint is MusicBrainz's alone, so a disc-ID failure names it.
    var failedSearch: FailedSearch? {
        switch self {
        case .discId: FailedSearch(source: .musicBrainz, step: .signal(.discId))
        case .barcode(let source, _):
            FailedSearch(source: source, step: .signal(.barcode))
        case .catalog(let source, _):
            FailedSearch(source: source, step: .signal(.catalog))
        case .search(let source, _):
            FailedSearch(source: source, step: .titleSearch)
        case .barcodeScan, .releaseDetails: nil
        }
    }
}
