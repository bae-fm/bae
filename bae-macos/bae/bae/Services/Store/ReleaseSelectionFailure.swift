import BaeKit

/// A failed release selection belongs to the row the person picked, which
/// `release`, the release the row shows first, identifies. It does not belong
/// to the candidate's metadata draft or to the search as a whole.
struct ReleaseSelectionFailure: Equatable {
    let release: BridgeLookupRef
    let error: DisplayError

    /// Whether `pressing` is the row the person picked when the selection
    /// failed.
    func matches(_ pressing: Pressing) -> Bool {
        pressing.link.record == release
    }
}

/// What one candidate's metadata pick is doing: the source still being read
/// into its draft, or how the last read of it failed.
enum CandidateMetadataApplication: Equatable {
    case applying(CandidateMetadataApplicationSession)
    case failed(ReleaseSelectionFailure)
}
