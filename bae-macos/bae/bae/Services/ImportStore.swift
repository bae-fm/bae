import BaeKit
import Combine
import OrderedCollections
import SwiftUI

/// The sidebar row list's sort order — the only thing left for a surface to
/// decide once core has placed every row into its tab and, under Needs you,
/// its group. `ordered(_:by:title:)` below interprets it against whatever
/// title a row is actually showing (the matched release's, or the folder
/// name), so the sort matches what a person reads.
///
/// Only name order survives this redesign: `TriageRow` carries no discovery
/// timestamp (`TriageQueue.rows` is ordered by watched folder then candidate
/// key, not by when a candidate was found), so a "date added" option would
/// silently degrade into an alias for name order. Better to drop it than keep
/// a control that lies about what it does.
enum CandidateSortOrder: String, CaseIterable {
    case nameAZ
    case nameZA
}

/// One row under the Skipped tab: a manually-skipped candidate, or an invalid
/// folder (looked like a release but failed validation). Both come off
/// `triageQueue` — core already decided both belong here — this only pairs
/// them into one list a view can iterate.
enum SkippedRow: Identifiable {
    case candidate(BridgeTriageRow)
    case invalid(BridgeInvalidCandidate)

    var id: String {
        switch self {
        case .candidate(let row): row.candidateKey
        case .invalid(let c): c.folderPath
        }
    }

    /// What the row sorts and filters by: the candidate's display title, or
    /// the invalid folder's source name.
    var sortTitle: String {
        switch self {
        case .candidate(let row): ImportStore.displayTitle(row)
        case .invalid(let c): c.sourceFolderName
        }
    }

    /// Extra filter text beyond `sortTitle`: the folder path, for a query
    /// that names disk structure rather than a title.
    var filterPath: String {
        switch self {
        case .candidate(let row): row.candidateKey
        case .invalid(let c): c.folderPath
        }
    }
}

/// Session state for the import flow. Mixed-writer: core drives event-driven
/// fields — scan/identify state through the import-candidate projections, the
/// triage queue through its own projection, preview state through the shared
/// event dispatcher — while views drive user-set fields (mode, selectedCover)
/// via `mutateCandidate(forKey:_:)`. The single-writer rule applies per field,
/// not per store.
///
/// One ordered dictionary per source: folder-scan candidates and re-identify
/// candidates. Keys are unique across both (folder path or
/// `reidentify:{releaseId}`), so cross-type helpers below can look up a
/// candidate by key without knowing its source.
@Observable
class ImportStore {
    var folderCandidates: OrderedDictionary<String, Candidate> = [:]
    /// The folders being watched for imports, in add order. Fetched when the
    /// import view appears and refreshed on watched-folder invalidation.
    var watchedFolders: [BridgeWatchedFolder] = []
    /// Re-identify candidates — one per active "Re-identify..." sheet.
    /// Keyed by `reidentify:{releaseId}` so identify events route the same
    /// way folder events do. The sheet inserts on open and removes on dismiss.
    var reIdentifyCandidates: OrderedDictionary<String, Candidate> = [:]

    /// The sidebar's pre-shaped rows, tab counts, and invalid folders — core's
    /// projection, read whole and re-read on the domains
    /// `AppService.makeImportTriageQueueProjection` registers for. Defaults to
    /// an empty queue rather than `nil`: "not loaded yet" and "the queue is
    /// genuinely empty" render identically (the tab's empty state), so no
    /// surface needs to tell them apart.
    var triageQueue: BridgeTriageQueue = BridgeTriageQueue(
        rows: [],
        invalid: [],
        counts: BridgeTriageTabCounts(
            ready: 0,
            needsYou: 0,
            done: 0,
            skipped: 0
        )
    )

    /// The queue sweep's identified-count over total, for the header's
    /// progress line and bar. `nil` before the first tick of a session — the
    /// header hides rather than opening on a bar frozen at zero.
    var queueIdentifyProgress: (identified: UInt32, total: UInt32)?

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
    /// the shared search/confirmation flow.
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
    }

    func applyImportCandidateSnapshot(
        key requestedKey: String,
        snapshot: BridgeImportCandidateSnapshot?
    ) {
        guard let snapshot else {
            folderCandidates.removeValue(forKey: requestedKey)
            return
        }

        switch snapshot {
        case .folder(let bridge, let runtime):
            let incoming = Candidate(bridge: bridge)
            let existing = folderCandidates[incoming.key]
            folderCandidates[incoming.key] =
                existing.map { incoming.withSessionState(from: $0) }
                ?? incoming
            applyRuntime(runtime, to: incoming.key)

        case .invalid(let candidate):
            // The sidebar reads invalid folders from `triageQueue.invalid`
            // now (one source of truth for the Skipped tab); this path only
            // still has to stop treating the key as an addressable candidate.
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
        candidate.signalsToolbar = runtime.signalsToolbar
        candidate.signals = runtime.signals.map(Signals.init(bridge:))
        candidate.importStatus = runtime.importStatus
    }

    func removeLibraryStatus(releaseId: String) {
        sweepCandidates { candidate in
            if case .complete(releaseId: let completedRelease, albumId: _) =
                candidate.importStatus,
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

// MARK: - Triage rendering

extension ImportStore {
    /// The title a row leads with — the matched release's, or the folder name
    /// when nothing matched. What the sort order and the filter match
    /// against, because it's what the row actually shows.
    static func displayTitle(_ row: BridgeTriageRow) -> String {
        row.matched?.title ?? row.folderName
    }

    /// The rows in `tab`, filtered by `filterText` and ordered by
    /// `sortOrder`. Tab membership is `placement`'s, computed once in core —
    /// this only filters and orders what core already placed. Used for Ready,
    /// Done, and Skipped's candidate half; Needs you uses
    /// `needsYouGroups(filterText:sortOrder:)` instead, since it groups by
    /// question rather than rendering one flat list.
    func rows(
        tab: BridgeTriageTab,
        filterText: String,
        sortOrder: CandidateSortOrder
    ) -> [BridgeTriageRow] {
        let scoped = triageQueue.rows.filter {
            bridgeTriageTab(placement: $0.placement) == tab
        }
        return Self.ordered(
            filtered(scoped, filterText: filterText),
            by: sortOrder,
            title: Self.displayTitle
        )
    }

    /// Needs-you rows grouped by the question they ask, in the order core
    /// declares (`bridgeNeedsYouGroupsInOrder`) — the one place that ordering
    /// is stated. A group with nothing left in it (everything filtered out, or
    /// everything answered) drops out rather than rendering an empty header.
    func needsYouGroups(
        filterText: String,
        sortOrder: CandidateSortOrder
    ) -> [(group: BridgeNeedsYouGroup, rows: [BridgeTriageRow])] {
        let needsYou = filtered(
            triageQueue.rows.filter {
                bridgeTriageTab(placement: $0.placement) == .needsYou
            },
            filterText: filterText
        )
        return bridgeNeedsYouGroupsInOrder()
            .compactMap { group in
                let rows = Self.ordered(
                    needsYou.filter { row in
                        guard case .needsYou(let rowGroup, _) = row.placement
                        else {
                            return false
                        }
                        return rowGroup == group
                    },
                    by: sortOrder,
                    title: Self.displayTitle
                )
                return rows.isEmpty ? nil : (group, rows)
            }
    }

    /// The Skipped tab: manually-skipped candidates and invalid folders as one
    /// filtered, ordered list — the tab shows both under a single header, so
    /// there is nothing to group by within it.
    func skippedRows(
        filterText: String,
        sortOrder: CandidateSortOrder
    ) -> [SkippedRow] {
        let candidateRows = triageQueue.rows
            .filter { bridgeTriageTab(placement: $0.placement) == .skipped }
            .map(SkippedRow.candidate)
        let invalidRows = triageQueue.invalid.map(SkippedRow.invalid)
        let all = candidateRows + invalidRows

        let query = filterText.lowercased()
        let matching =
            query.isEmpty
            ? all
            : all.filter {
                $0.sortTitle.lowercased().contains(query)
                    || $0.filterPath.lowercased().contains(query)
            }
        return Self.ordered(matching, by: sortOrder, title: \.sortTitle)
    }

    /// Order `rows` by `sortOrder`, keying the name comparison off `title`.
    /// One dispatch for triage rows (by display title) and skipped rows (by
    /// title or invalid-folder name), so a new `CandidateSortOrder` case is
    /// handled in one place.
    private static func ordered<T>(
        _ rows: [T],
        by sortOrder: CandidateSortOrder,
        title: (T) -> String
    ) -> [T] {
        switch sortOrder {
        case .nameAZ:
            return rows.sorted {
                title($0).localizedCaseInsensitiveCompare(title($1))
                    == .orderedAscending
            }
        case .nameZA:
            return rows.sorted {
                title($0).localizedCaseInsensitiveCompare(title($1))
                    == .orderedDescending
            }
        }
    }

    /// Filter `rows` by `filterText` against the display title, the folder
    /// name, and the candidate key (folder path) — a query can name either
    /// what matched or the folder on disk.
    private func filtered(
        _ rows: [BridgeTriageRow],
        filterText: String
    ) -> [BridgeTriageRow] {
        let query = filterText.lowercased()
        guard !query.isEmpty else {
            return rows
        }
        return rows.filter {
            Self.displayTitle($0).lowercased().contains(query)
                || $0.folderName.lowercased().contains(query)
                || $0.candidateKey.lowercased().contains(query)
        }
    }
}
