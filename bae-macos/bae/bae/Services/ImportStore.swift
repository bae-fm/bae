import Combine
import OrderedCollections
import SwiftUI

/// The three state tabs the import candidate list groups candidates under.
/// `tab(for:)` assigns each candidate to exactly one.
enum CandidateTab: Hashable {
    case new
    case added
    case skipped
}

/// One row under the Skipped tab: a manually-skipped candidate, or an invalid
/// folder (looked like a release but failed validation). The view renders each
/// case with its own row; the data layer decides which folder a row belongs to.
enum SkippedRow: Identifiable {
    case candidate(Candidate)
    case invalid(BridgeInvalidCandidate)

    var id: String {
        switch self {
        case .candidate(let c): c.key
        case .invalid(let c): c.folderPath
        }
    }
}

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
    /// Folders that look like a release but failed validation, keyed by folder
    /// path. They carry no files or identify state — they can't be imported —
    /// only a reason. Surfaced under the Skipped tab with a warning. A folder is
    /// never in both `folderCandidates` and here: the reducer moves it across as
    /// validity flips.
    var invalidCandidates: OrderedDictionary<String, BridgeInvalidCandidate> =
        [:]
    /// The folders being watched for imports, in add order. The candidate list
    /// renders one collapsible group per folder; each candidate's
    /// `watchedFolderPath` matches one of these `path`s. Fetched when the import
    /// view appears and refreshed on add/remove via `WatchedFoldersChanged`.
    var watchedFolders: [BridgeWatchedFolder] = []
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

    /// The state tab a candidate belongs to. Skipped wins (a skipped candidate
    /// stays under Skipped even if it was also imported); otherwise a candidate
    /// that completed an import this session or matches an already-imported
    /// folder is Added; everything else is New.
    func tab(for candidate: Candidate) -> CandidateTab {
        if candidate.skipped {
            return .skipped
        }
        if case .complete = candidate.importStatus {
            return .added
        }
        if candidate.isAdded {
            return .added
        }
        return .new
    }

    /// Folder candidates grouped for the candidate list: one entry per watched
    /// folder (in add order), each holding that folder's candidates that belong
    /// to `tab`, filtered by `filterText` and ordered by `sortOrder`. Candidates
    /// whose watched folder is no longer watched are excluded, and — while
    /// filtering — folders with no matches drop out. The view iterates and
    /// renders this; the tab-assignment, filtering, sorting, and grouping live
    /// here in the state layer.
    func candidateGroups(
        tab: CandidateTab,
        filterText: String,
        sortOrder: CandidateSortOrder
    )
        -> [(folder: BridgeWatchedFolder, candidates: [Candidate])]
    {
        let query = filterText.lowercased()
        return watchedFolders.compactMap { folder in
            var rows = folderCandidates.values.filter {
                $0.watchedFolderPath == folder.path && self.tab(for: $0) == tab
            }
            if !query.isEmpty {
                rows = rows.filter {
                    $0.displayName.lowercased().contains(query)
                        || $0.key.lowercased().contains(query)
                }
                if rows.isEmpty {
                    return nil
                }
            }
            let ordered = Self.ordered(
                rows,
                by: sortOrder,
                name: \.displayName
            )
            return (folder, ordered)
        }
    }

    /// Rows for the Skipped tab, grouped per watched folder: each folder's
    /// manually-skipped candidates plus the invalid candidates scanned from it,
    /// filtered by `filterText` (display name or path) and ordered by
    /// `sortOrder`. Folders with no matching rows drop out. The view iterates and
    /// renders this; the grouping/filtering/sorting lives here in the data layer.
    func skippedGroups(filterText: String, sortOrder: CandidateSortOrder)
        -> [(folder: BridgeWatchedFolder, rows: [SkippedRow])]
    {
        let query = filterText.lowercased()
        let skippedByFolder = candidateGroups(
            tab: .skipped,
            filterText: filterText,
            sortOrder: sortOrder
        )
        return watchedFolders.compactMap { folder in
            let candidateRows: [SkippedRow] =
                (skippedByFolder.first { $0.folder.path == folder.path }?
                .candidates ?? [])
                .map(SkippedRow.candidate)

            var invalid = invalidCandidates.values.filter {
                $0.watchedFolderPath == folder.path
            }
            if !query.isEmpty {
                invalid = invalid.filter {
                    $0.sourceFolderName.lowercased().contains(query)
                        || $0.folderPath.lowercased().contains(query)
                }
            }
            let invalidRows: [SkippedRow] =
                Self.ordered(invalid, by: sortOrder, name: \.sourceFolderName)
                .map(SkippedRow.invalid)

            let rows = candidateRows + invalidRows
            return rows.isEmpty ? nil : (folder, rows)
        }
    }

    /// Per-tab candidate counts across every folder candidate (independent of
    /// the active filter), for the tab bar's count badges. A candidate whose
    /// watched folder is no longer present still counts — the watcher drops such
    /// candidates from the store, so the store only holds live ones. Invalid
    /// candidates count under Skipped alongside manually-skipped ones.
    func candidateTabCounts() -> (new: Int, added: Int, skipped: Int) {
        var counts = (new: 0, added: 0, skipped: 0)
        for candidate in folderCandidates.values {
            switch tab(for: candidate) {
            case .new: counts.new += 1
            case .added: counts.added += 1
            case .skipped: counts.skipped += 1
            }
        }
        counts.skipped += invalidCandidates.count
        return counts
    }

    /// Order `rows` by `sortOrder`, keying the name cases off `name`. One
    /// dispatch for both candidate rows (by display name) and invalid rows (by
    /// folder name), so a new `CandidateSortOrder` case is handled in one place.
    private static func ordered<T>(
        _ rows: [T],
        by sortOrder: CandidateSortOrder,
        name: (T) -> String
    ) -> [T] {
        switch sortOrder {
        case .nameAZ:
            return rows.sorted {
                name($0).localizedCaseInsensitiveCompare(name($1))
                    == .orderedAscending
            }
        case .nameZA:
            return rows.sorted {
                name($0).localizedCaseInsensitiveCompare(name($1))
                    == .orderedDescending
            }
        case .dateAddedNewest:
            return rows
        case .dateAddedOldest:
            return rows.reversed()
        }
    }
}
