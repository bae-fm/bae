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
/// timestamp, so a "date added" option would
/// silently degrade into an alias for name order. Better to drop it than keep
/// a control that lies about what it does.
enum CandidateSortOrder: String, CaseIterable {
    case nameAZ
    case nameZA
}

struct ReleaseQueueEntry: Identifiable {
    let id: String
    let bridge: BridgeTriageEntry

    init(_ bridge: BridgeTriageEntry) {
        self.bridge = bridge
        id =
            switch bridge {
            case .candidate(let stableKey, _),
                .boundary(let stableKey, _),
                .invalid(let stableKey, _):
                stableKey
            }
    }
}

struct ReleaseQueueSectionID: Hashable {
    let tab: BridgeTriageTab
    let watchedFolderPath: String
    let groupRelativeFolderPath: String?

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.tab == rhs.tab
            && lhs.watchedFolderPath == rhs.watchedFolderPath
            && lhs.groupRelativeFolderPath == rhs.groupRelativeFolderPath
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(tab)
        hasher.combine(watchedFolderPath)
        hasher.combine(groupRelativeFolderPath)
    }
}

struct ReleaseQueueSection: Identifiable {
    let tab: BridgeTriageTab
    let watchedFolderPath: String
    let group: BridgeTriageGroup?
    let entries: [ReleaseQueueEntry]

    var id: ReleaseQueueSectionID {
        ReleaseQueueSectionID(
            tab: tab,
            watchedFolderPath: watchedFolderPath,
            groupRelativeFolderPath: group?.key.relativeFolderPath
        )
    }
}

/// Session state for the import flow. Mixed-writer: core drives scan/identify,
/// the triage queue, and preview state through value subscriptions, while views
/// drive user-set fields
/// (mode, coverPick)
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
    /// import view appears and updates through the candidate subscription.
    var watchedFolders: [BridgeWatchedFolder] = []
    /// Re-identify candidates — one per active "Re-identify..." sheet.
    /// Keyed by `reidentify:{releaseId}` so identify events route the same
    /// way folder events do. The sheet inserts on open and removes on dismiss.
    var reIdentifyCandidates: OrderedDictionary<String, Candidate> = [:]

    /// The sidebar's pre-shaped sections and tab counts — core's projection,
    /// delivered whole whenever its database or candidate inputs change.
    /// Defaults to
    /// an empty queue rather than `nil`: "not loaded yet" and "the queue is
    /// genuinely empty" render identically (the tab's empty state), so no
    /// surface needs to tell them apart.
    var triageQueue: BridgeTriageQueue = BridgeTriageQueue(
        sections: [],
        counts: BridgeTriageTabCounts(
            ready: 0,
            needsYou: 0,
            done: 0,
            skipped: 0
        ),
        folderScanStatuses: []
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
        for snapshot in snapshot.runtimeCandidates {
            mutateReIdentifyCandidate(key: snapshot.key) { candidate in
                applyRuntime(snapshot.runtime, to: &candidate)
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
    func releaseSections(
        tab: BridgeTriageTab,
        filterText: String,
        sortOrder: CandidateSortOrder
    ) -> [ReleaseQueueSection] {
        triageQueue.sections
            .filter { $0.tab == tab }
            .compactMap { section in
                let entries = filteredEntries(
                    section.entries,
                    filterText: filterText,
                    sortOrder: sortOrder
                )
                guard !entries.isEmpty else { return nil }
                return ReleaseQueueSection(
                    tab: section.tab,
                    watchedFolderPath: section.watchedFolderPath,
                    group: section.group,
                    entries: entries.map(ReleaseQueueEntry.init)
                )
            }
    }

    private func filteredEntries(
        _ entries: [BridgeTriageEntry],
        filterText: String,
        sortOrder: CandidateSortOrder
    ) -> [BridgeTriageEntry] {
        let query = filterText.lowercased()
        let matching = entries.filter { entry in
            guard !query.isEmpty else { return true }
            switch entry {
            case .candidate(_, let row):
                return Self.displayTitle(row).lowercased().contains(query)
                    || row.displayPath.lowercased().contains(query)
            case .boundary(_, let boundary):
                return boundary.name.lowercased().contains(query)
                    || boundary.displayPath.lowercased().contains(query)
                    || boundary.treeRows.contains {
                        $0.displayPath.lowercased().contains(query)
                    }
            case .invalid(_, let candidate):
                return candidate.sourceFolderName.lowercased().contains(query)
                    || candidate.displayPath.lowercased().contains(query)
            }
        }
        return matching.sorted { left, right in
            let leftTitle = entryTitle(left)
            let rightTitle = entryTitle(right)
            let order = leftTitle.localizedCaseInsensitiveCompare(rightTitle)
            return sortOrder == .nameAZ
                ? order == .orderedAscending : order == .orderedDescending
        }
    }

    private func entryTitle(_ entry: BridgeTriageEntry) -> String {
        switch entry {
        case .candidate(_, let row): Self.displayTitle(row)
        case .boundary(_, let boundary): boundary.name
        case .invalid(_, let candidate): candidate.sourceFolderName
        }
    }

    /// The title a row leads with — the matched release's, or the folder name
    /// when nothing matched. What the sort order and the filter match
    /// against, because it's what the row actually shows.
    static func displayTitle(_ row: BridgeTriageRow) -> String {
        row.matched?.title ?? row.folderName
    }

    /// Whether `displayTitle` fell through to the folder name — the rows that
    /// take a folder icon, so the title reads as a place on disk rather than a
    /// release nobody has matched.
    static func titleIsFolderName(_ row: BridgeTriageRow) -> Bool {
        row.matched == nil
    }

    func triageRow(forKey key: String) -> BridgeTriageRow? {
        triageQueue.sections.lazy
            .flatMap(\.entries)
            .compactMap { entry in
                guard case .candidate(_, let row) = entry else { return nil }
                return row
            }
            .first { $0.candidateKey == key }
    }

    /// The first row the identify count is still waiting on — a candidate with
    /// no verdict yet, whichever phase it is in. `nil` when the count has
    /// nothing left to wait on.
    ///
    /// This is what the header's line points at: the number moves on its own,
    /// but while it is short of its total there is a row somewhere behind it,
    /// and the line is the only place that knows there is.
    var firstUnidentifiedCandidateKey: String? {
        triageQueue.sections.lazy
            .flatMap(\.entries)
            .compactMap { entry -> BridgeTriageRow? in
                guard case .candidate(_, let row) = entry else { return nil }
                return row
            }
            .first { row in
                if case .needsYou(_, .stillIdentifying) = row.placement {
                    return true
                }
                return false
            }?
            .candidateKey
    }

    /// The cover art of every Ready row, in queue order.
    ///
    /// Identification already fetched these — a row reaches Ready by settling
    /// on one match, and that match carries the thumbnail URL — so the Ready
    /// tab has no reason to open on a grid of spinners. Read unfiltered and
    /// unsorted: what the tab is about to draw does not depend on what the
    /// filter box currently says.
    var readyCoverThumbnailUrls: [String] {
        triageQueue.sections
            .filter { $0.tab == .ready }
            .flatMap(\.entries)
            .compactMap { entry in
                guard case .candidate(_, let row) = entry else { return nil }
                return row.matched?.coverThumbnailUrl
            }
    }

    func selectableReadyRows(
        filterText: String,
        sortOrder: CandidateSortOrder
    ) -> [BridgeTriageRow] {
        releaseSections(
            tab: .ready,
            filterText: filterText,
            sortOrder: sortOrder
        )
        .flatMap(\.entries)
        .compactMap { entry in
            guard
                case .candidate(_, let row) = entry.bridge,
                row.selectable
            else {
                return nil
            }
            return row
        }
    }
}
