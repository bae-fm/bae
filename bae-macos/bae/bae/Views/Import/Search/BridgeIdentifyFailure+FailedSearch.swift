import BaeKit

/// One lookup a provider was asked for: which step of identification, at
/// which source. A failure names one; the same source may well have answered
/// the other steps, and those matches are on the list.
struct FailedSearch: Hashable {
    let source: BridgeMetadataSource
    let step: BridgeSignalKind
}

extension BridgeIdentifyFailure {
    /// The lookup this failure names, for the line saying its results are
    /// missing from the list. `nil` for the steps no provider owns. The
    /// disc-ID endpoint is MusicBrainz's alone, so a disc-ID failure names it.
    var failedSearch: FailedSearch? {
        switch self {
        case .discId: FailedSearch(source: .musicBrainz, step: .discId)
        case .barcode(let source, _):
            FailedSearch(source: source, step: .barcode)
        case .catalog(let source, _):
            FailedSearch(source: source, step: .catalog)
        case .barcodeScan, .releaseDetails: nil
        }
    }
}
