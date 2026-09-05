import BaeKit

/// A failed release selection belongs to the provider result that was read,
/// not to the candidate's metadata draft or to the search as a whole.
struct ReleaseSelectionFailure: Equatable {
    let release: BridgeMetadataRef
    let message: String

    func matches(_ pressing: Pressing) -> Bool {
        pressing.releases.contains {
            $0.source == release.source && $0.releaseId == release.releaseId
        }
    }
}

enum CandidateMetadataApplication: Equatable {
    case applying(CandidateMetadataApplicationSession)
    case failed(ReleaseSelectionFailure)
}
