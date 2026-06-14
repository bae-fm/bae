import Combine
import OrderedCollections
import SwiftUI

/// Session state for the import flow. Mixed-writer: the reducer drives
/// event-driven fields (scan, identify, preview state), while views drive
/// user-set fields (mode, selectedCoverUrl) via `mutateCandidate(forKey:_:)`.
/// The single-writer rule applies per field, not per store.
///
/// One ordered dictionary per source: folder-scan candidates and re-identify
/// candidates. Keys are unique across both (folder path or
/// `reidentify:{releaseId}`), so cross-type helpers below can look up a
/// candidate by key without knowing its source.
@Observable
class ImportStore {
    var folderCandidates: OrderedDictionary<String, Candidate> = [:]
    /// Re-identify candidates — one per active "Re-identify..." sheet.
    /// Keyed by `reidentify:{releaseId}` so identify events route the same
    /// way folder events do. The sheet inserts on open and removes on dismiss.
    var reIdentifyCandidates: OrderedDictionary<String, Candidate> = [:]

    var previewState: PreviewState = .idle

    /// Preview audio progress (the import-tab preview player).
    /// High-frequency — published as a Combine signal so only the
    /// progress-bar NSView re-renders. Buffers the latest value so a
    /// subscriber created mid-session (e.g. after the overlay's NSView is
    /// rebuilt) sees the current position immediately instead of an empty
    /// bar until the next tick.
    @ObservationIgnored
    let previewProgressSubject = CurrentValueSubject<
        PreviewProgressEvent, Never
    >(.reset)

    /// Look up a candidate by key across both source dicts. Used when the
    /// caller doesn't know (or doesn't care about) the candidate's source —
    /// e.g. generic reducer cases and the shared search/confirmation flow.
    func candidate(forKey key: String) -> Candidate? {
        folderCandidates[key] ?? reIdentifyCandidates[key]
    }

    /// Invalidate cached "in library" state after an album is removed from
    /// the library. Candidates cache library facts in three places —
    /// `importStatus == .complete`, the search-merged `libraryStatuses`, and
    /// the statuses embedded in terminal `identifyState` payloads — and none
    /// of them receive library events on their own; without this sweep a
    /// deleted release keeps its green "Imported" check, "View in Library"
    /// link, and disabled commit buttons. Derived purely from the removal
    /// event's ids.
    func handleAlbumRemoved(albumId: String, releaseIds: [String]) {
        let removedReleases = Set(releaseIds)
        sweepCandidates { candidate in
            if case .complete(let completedAlbum, let completedRelease) =
                candidate.importStatus,
                completedAlbum == albumId
                    || removedReleases.contains(completedRelease)
            {
                candidate.importStatus = nil
            }
            candidate.removeLibraryStatuses { releaseId, status in
                removedReleases.contains(releaseId) || status.albumId == albumId
            }
        }
    }

    /// Companion of `handleAlbumRemoved(albumId:releaseIds:)` for a release
    /// removal that leaves its album in place.
    func handleReleaseRemoved(releaseId: String) {
        sweepCandidates { candidate in
            if case .complete(_, let completedRelease) = candidate.importStatus,
                completedRelease == releaseId
            {
                candidate.importStatus = nil
            }
            candidate.removeLibraryStatuses { id, _ in id == releaseId }
        }
    }

    private func sweepCandidates(_ mutate: (inout Candidate) -> Void) {
        func sweep(
            _ dict: inout OrderedDictionary<String, Candidate>
        ) {
            for key in dict.keys {
                mutate(&dict[key]!)
            }
        }
        sweep(&folderCandidates)
        sweep(&reIdentifyCandidates)
    }

    /// Mutate a candidate in-place in whichever dict holds it. No-op if the
    /// key isn't present in any dict.
    func mutateCandidate(
        forKey key: String,
        _ mutate: (inout Candidate) -> Void
    ) {
        if folderCandidates[key] != nil {
            var c = folderCandidates[key]!
            mutate(&c)
            folderCandidates[key] = c
            return
        }
        if reIdentifyCandidates[key] != nil {
            var c = reIdentifyCandidates[key]!
            mutate(&c)
            reIdentifyCandidates[key] = c
        }
    }
}
