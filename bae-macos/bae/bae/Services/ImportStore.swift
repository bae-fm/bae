import BaeKit
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

struct CandidateTabCounts {
    var new: Int
    var added: Int
    var skipped: Int
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
/// user-set fields (mode, selectedCover) via `mutateCandidate(forKey:_:)`.
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
    /// view appears and refreshed on watched-folder invalidation.
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

    /// Per-track loudness measurement progress during an import. High-frequency
    /// (one per track), published as a Combine signal so only the leaf bar in
    /// the confirm pane re-renders — never the candidate row. Carries the
    /// candidate key so a leaf filters to its own import; `nil` until the first
    /// tick. Buffers the latest value for a leaf created mid-pass.
    @ObservationIgnored
    let importLoudnessSubject = CurrentValueSubject<
        ImportLoudnessProgressEvent?, Never
    >(nil)

    /// Look up a candidate by key across both source dicts. Used when the
    /// caller doesn't know (or doesn't care about) the candidate's source —
    /// e.g. generic reducer cases and the shared search/confirmation flow.
    func candidate(forKey key: String) -> Candidate? {
        folderCandidates[key] ?? reIdentifyCandidates[key]
    }

    func applyImportCandidatesSnapshot(
        _ snapshot: BridgeImportCandidatesSnapshot
    ) {
        watchedFolders = snapshot.watchedFolders

        var nextFolderCandidates: OrderedDictionary<String, Candidate> = [:]
        for bridge in snapshot.folderCandidates {
            var incoming = Candidate(bridge: bridge.candidate)
            applyRuntime(bridge.runtime, to: &incoming)
            if let existing = folderCandidates[incoming.key] {
                nextFolderCandidates[incoming.key] = incoming.withSessionState(
                    from: existing
                )
            }
            else {
                nextFolderCandidates[incoming.key] = incoming
            }
        }
        folderCandidates = nextFolderCandidates

        invalidCandidates = OrderedDictionary(
            uniqueKeysWithValues: snapshot.invalidCandidates.map {
                ($0.folderPath, $0)
            }
        )
    }

    func applyImportCandidateSnapshot(
        key requestedKey: String,
        snapshot: BridgeImportCandidateSnapshot?
    ) {
        guard let snapshot else {
            folderCandidates.removeValue(forKey: requestedKey)
            invalidCandidates.removeValue(forKey: requestedKey)
            return
        }

        switch snapshot {
        case .folder(let bridge, let runtime):
            let incoming = Candidate(bridge: bridge)
            let existing = folderCandidates[incoming.key]
            folderCandidates[incoming.key] =
                existing.map { incoming.withSessionState(from: $0) }
                ?? incoming
            invalidCandidates.removeValue(forKey: incoming.key)
            applyRuntime(runtime, to: incoming.key)

        case .invalid(let candidate):
            invalidCandidates[candidate.folderPath] = candidate
            folderCandidates.removeValue(forKey: candidate.folderPath)

        case .runtime(let key, let runtime):
            applyRuntime(runtime, to: key)
        }
    }

    private func applyRuntime(
        _ runtime: BridgeCandidateRuntimeSnapshot,
        to key: String
    ) {
        if key.hasPrefix("reidentify:") {
            mutateReIdentifyCandidate(key: key) { candidate in
                applyRuntime(runtime, to: &candidate)
            }
        }
        else {
            mutateFolderCandidate(key: key) { candidate in
                applyRuntime(runtime, to: &candidate)
            }
        }
    }

    private func applyRuntime(
        _ runtime: BridgeCandidateRuntimeSnapshot,
        to candidate: inout Candidate
    ) {
        candidate.identifyState = IdentifyState(
            bridge: runtime.identifyState
        )
        candidate.signalsToolbar = SignalsToolbar(
            bridge: runtime.signalsToolbar
        )
        candidate.signals = runtime.signals.map(Signals.init(bridge:))
        candidate.importStatus = runtime.importStatus.map(
            ImportStatus.init(bridge:)
        )
    }

    func removeLibraryStatus(releaseId: String) {
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
                if var c = dict[key] {
                    mutate(&c)
                    dict[key] = c
                }
            }
        }
        sweep(&folderCandidates)
        sweep(&reIdentifyCandidates)
    }

    private func mutateFolderCandidate(
        key: String,
        _ mutate: (inout Candidate) -> Void
    ) {
        if var candidate = folderCandidates[key] {
            mutate(&candidate)
            folderCandidates[key] = candidate
        }
    }

    private func mutateReIdentifyCandidate(
        key: String,
        _ mutate: (inout Candidate) -> Void
    ) {
        if var candidate = reIdentifyCandidates[key] {
            mutate(&candidate)
            reIdentifyCandidates[key] = candidate
        }
    }

    func mutateCandidate(
        forKey key: String,
        _ mutate: (inout Candidate) -> Void
    ) {
        if key.hasPrefix("reidentify:") {
            mutateReIdentifyCandidate(key: key, mutate)
        }
        else {
            mutateFolderCandidate(key: key, mutate)
        }
    }
}

extension ImportStore {
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
    func candidateTabCounts() -> CandidateTabCounts {
        var counts = CandidateTabCounts(new: 0, added: 0, skipped: 0)
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
