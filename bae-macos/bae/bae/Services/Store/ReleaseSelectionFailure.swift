import BaeKit

/// A failed release selection belongs to the provider result that was read,
/// not to the candidate's metadata draft or to the search as a whole.
struct ReleaseSelectionFailure: Equatable {
    let release: BridgeMetadataRef
    let error: DisplayError

    func matches(_ pressing: Pressing) -> Bool {
        pressing.releases.contains {
            $0.source == release.catalog && $0.releaseId == release.key
        }
    }
}

/// What one candidate's metadata pick is doing: the source still being read
/// into its draft, or how the last read of it failed.
enum CandidateMetadataApplication: Equatable {
    case applying(CandidateMetadataApplicationSession)
    case failed(ReleaseSelectionFailure)
}
