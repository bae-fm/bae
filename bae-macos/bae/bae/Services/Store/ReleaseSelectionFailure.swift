import BaeKit

/// A failed release selection belongs to the row that was picked, named by
/// the record it leads with, not to the candidate's metadata draft or to the
/// search as a whole.
struct ReleaseSelectionFailure: Equatable {
    let release: BridgeLookupRef
    let error: DisplayError

    /// Whether `pressing` is the row the failed pick was made on.
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
