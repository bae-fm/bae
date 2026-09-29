import BaeKit

/// One lookup a provider was asked for: which step of identification, at
/// which source. A failure names one; the same source may well have answered
/// the other steps, and those matches are on the list.
struct FailedSearch: Hashable {
    let source: BridgeCatalog
    let step: Step

    /// The steps a provider answers. Most are the identifiers the badge row
    /// names; the title search is the run's own last step, which has no badge
    /// because it is not a value the folder carries.
    enum Step: Hashable {
        case signal(BridgeSignalKind)
        case titleSearch
    }
}

extension BridgeIdentifyFailure {
    /// The lookup this failure names, for the line saying its results are
    /// missing from the list. `nil` for the fetches that apply the release a
    /// run picked — its details, artist images and cover — which leave no
    /// results missing. Only MusicBrainz is asked about disc IDs and ISRCs,
    /// so their failures name it.
    var failedSearch: FailedSearch? {
        switch self {
        case .discId: FailedSearch(source: .musicBrainz, step: .signal(.discId))
        case .barcode(let source, _):
            FailedSearch(source: source, step: .signal(.barcode))
        case .catalog(let source, _):
            FailedSearch(source: source, step: .signal(.catalog))
        case .isrc: FailedSearch(source: .musicBrainz, step: .signal(.isrc))
        case .search(let source, _):
            FailedSearch(source: source, step: .titleSearch)
        case .releaseDetails, .artistImages, .cover: nil
        }
    }
}
