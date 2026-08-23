import BaeKit
import Combine
import OrderedCollections
import SwiftUI

/// Session state for the import flow. Mixed-writer: core drives the list, the
/// per-candidate reads and preview state through value subscriptions, while
/// views drive user-set fields (mode, selectedCover) via
/// `mutateCandidate(forKey:_:)`. The single-writer rule applies per field, not
/// per store.
///
/// The list itself is paged: `items` holds the entries the sidebar has loaded,
/// keyed by the stable key core computes, and `selectedCandidates` holds the
/// whole folder for each row the user selected. Keys are unique across the
/// selected folders and the re-identify sessions (a folder path or
/// `reidentify:{releaseId}`), so the cross-type helpers below look a candidate
/// up by key without knowing its source.
@Observable
class ImportStore {
    /// The list entries the sidebar has loaded, by stable key. The paged list
    /// holds the order; this holds what each key renders as.
    var items: [String: BridgeImportListItem] = [:]

    /// Everything the chrome around the list shows: the tab counts, the
    /// watched folders and their scan statuses, the group keys, the Ready set,
    /// and the row the identify count is still waiting on. Defaults to an
    /// empty summary rather than `nil`: "not loaded yet" and "the queue is
    /// genuinely empty" render identically, so no surface needs to tell them
    /// apart.
    var summary: BridgeImportQueueSummary = BridgeImportQueueSummary(
        counts: BridgeTriageTabCounts(pending: 0, done: 0, skipped: 0),
        watchedFolders: [],
        folderScanStatuses: [],
        groupKeys: [],
        ready: [],
        firstUnidentifiedKey: nil
    )

    /// The selected rows' folders, read by key. A selection opens one
    /// subscription per key; the key leaves when its read says the folder is
    /// gone.
    var selectedCandidates: OrderedDictionary<String, Candidate> = [:]

    /// Re-identify candidates — one per active "Re-identify..." sheet.
    /// Keyed by `reidentify:{releaseId}` so identify events route the same
    /// way folder events do. The sheet inserts on open and removes on dismiss.
    var reIdentifyCandidates: OrderedDictionary<String, Candidate> = [:]

    /// The folders being watched for imports, in add order.
    var watchedFolders: [BridgeWatchedFolder] {
        summary.watchedFolders
    }

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

    /// What every candidate has in flight — a run's identify state, a running
    /// import's progress — as one signal. Not a stored dictionary: a leaf that
    /// draws one key subscribes and filters, the way the loudness bar does, so
    /// a progress tick redraws that leaf rather than the sidebar. The latest
    /// value per key lives in core, and a subscriber reads its own key from
    /// there when it appears.
    @ObservationIgnored
    let candidateRuntimeSubject = PassthroughSubject<
        BridgeCandidateRuntimeChange, Never
    >()

    /// Per-track loudness measurement progress during an import. High-frequency
    /// while each track decodes, published as a Combine signal so only the leaf
    /// bar in the confirm pane re-renders — never the candidate row. Carries the
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
        selectedCandidates[key] ?? reIdentifyCandidates[key]
    }

    // MARK: - The paged list

    /// Hold one page of list entries. Called by the list as each page lands.
    func ingest(_ entries: [BridgeImportListItem]) {
        for entry in entries {
            items[entry.id] = entry
        }
    }

    /// Drop entries the list no longer holds a position for, so a page the
    /// list evicted stops occupying memory.
    func retainItems(_ loadedKeys: [String]) {
        let loaded = Set(loadedKeys)
        items = items.filter { loaded.contains($0.key) }
    }

    /// The candidate's selected/default cover, or the queue's match thumbnail
    /// before identification has supplied one.
    func sidebarCover(for row: BridgeTriageRow) -> ImageContent? {
        candidate(forKey: row.candidateKey)?.cover?.thumbnailContent
            ?? row.matched?.coverThumbnailUrl.map { .remote(url: $0) }
    }

    /// The title a row leads with — the matched release's, or the folder name
    /// when nothing matched.
    static func displayTitle(_ row: BridgeTriageRow) -> String {
        row.matched?.title ?? row.folderName
    }

    /// Whether `displayTitle` fell through to the folder name — the rows that
    /// take a folder icon, so the title reads as a place on disk rather than a
    /// release nobody has matched.
    static func titleIsFolderName(_ row: BridgeTriageRow) -> Bool {
        row.matched == nil
    }

    // MARK: - Per-key reads

    /// One selected candidate, as its own read describes it, keeping whatever
    /// work the session has done on it.
    func applyCandidateDetail(
        key: String,
        detail: BridgeImportCandidateDetail
    ) {
        var incoming = Candidate(detail: detail)
        if let existing = selectedCandidates[key] {
            incoming = incoming.withSessionState(from: existing)
        }
        selectedCandidates[key] = incoming
    }

    private func mutateSelectedCandidate(
        key: String,
        _ mutate: (inout Candidate) -> Void
    ) {
        if var candidate = selectedCandidates[key] {
            mutate(&candidate)
            selectedCandidates[key] = candidate
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
            mutateSelectedCandidate(key: key, mutate)
        }
    }

    @MainActor
    func refreshLibraryStatusSubscriptions(
        importer: Importer,
        key: String
    ) {
        guard let current = candidate(forKey: key) else { return }
        let desired = current.search.libraryStatusSubscriptionKeys()

        mutateCandidate(forKey: key) { candidate in
            candidate.libraryStatusSubscriptions =
                candidate.libraryStatusSubscriptions.filter {
                    desired.contains($0.key)
                }
        }

        for statusKey in desired {
            guard
                candidate(forKey: key)?
                    .libraryStatusSubscriptions[statusKey] == nil
            else { continue }
            let observation = ReleaseLibraryStatusObservation()
            let identity = observation.identity
            mutateCandidate(forKey: key) {
                $0.libraryStatusSubscriptions[statusKey] = observation
            }
            let subscription = importer.subscribeReleaseLibraryStatus(
                source: statusKey.source,
                releaseId: statusKey.releaseId,
                sourceGroupId: statusKey.sourceGroupId,
                onValue: { [weak self] status in
                    guard
                        self?.candidate(forKey: key)?
                            .libraryStatusSubscriptions[statusKey]?
                            .identity == identity
                    else { return }
                    self?
                        .mutateCandidate(forKey: key) {
                            $0.libraryStatuses[status.releaseId] = status
                        }
                },
                onError: { [weak self] error in
                    guard let line = error.displayLine else { return }
                    guard
                        self?.candidate(forKey: key)?
                            .libraryStatusSubscriptions[statusKey]?
                            .identity == identity
                    else { return }
                    self?
                        .mutateCandidate(forKey: key) {
                            $0.error = line
                        }
                }
            )
            observation.install(subscription)
        }
    }
}
