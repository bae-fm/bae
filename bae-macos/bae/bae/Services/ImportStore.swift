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
    ///
    /// Written through `applySummary`, which drops a delivery that says the
    /// same thing as the one before it: every verdict the sweep commits
    /// re-delivers this whole value, and an equal write would still invalidate
    /// everything reading it.
    private(set) var summary = BridgeImportQueueSummary(
        counts: BridgeTriageTabCounts(pending: 0, done: 0, skipped: 0),
        watchedFolders: [],
        folderScanStatuses: [],
        groupKeys: [],
        ready: [],
        firstUnidentified: nil
    )

    /// The fault each watched root was last reported as having. A summary is
    /// re-delivered on every verdict the sweep commits, and a root that cannot
    /// be read fails the same way on every re-scan the timer starts, so this is
    /// what keeps one broken folder from raising one alert every time either
    /// happens. Rebuilt from each delivery, so a root that reads cleanly again
    /// — or stops being watched — leaves, and its next break is news.
    @ObservationIgnored
    private var reportedScanFailures: [String: String] = [:]

    /// Called for each watched root whose scan has newly failed, with the
    /// folder's path and the untranslated fault. Set by whoever owns an error
    /// surface — this store has none.
    ///
    /// The failure arrives on the summary rather than as a transient event
    /// because that is where it lives: the scan writes it to
    /// `folder_scan_roots` and the list's live query reads it back, so it is
    /// delivered whenever the UI subscribes rather than only to whoever was
    /// already listening. A scan that failed during launch, before any of this
    /// existed, is in the first delivery.
    @ObservationIgnored
    var onScanFailure:
        ((_ watchedFolderPath: String, _ detail: String) -> Void)?

    /// Take a delivered summary, unless it is the one already held.
    func applySummary(_ next: BridgeImportQueueSummary) {
        guard next != summary else { return }
        summary = next
        reportNewScanFailures()
    }

    private func reportNewScanFailures() {
        var current: [String: String] = [:]
        for status in summary.folderScanStatuses {
            guard case .failed(let detail) = status.status else { continue }
            current[status.watchedFolderPath] = detail
            if reportedScanFailures[status.watchedFolderPath] != detail {
                onScanFailure?(status.watchedFolderPath, detail)
            }
        }
        reportedScanFailures = current
    }

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

    /// Each candidate's extracted signals as extraction settles them. A
    /// signal rather than a stored value for the same reason the runtime is
    /// one: OCR reports several times per candidate, one form reads the
    /// result, and that form filters this to its own key. The latest value per
    /// key lives in core, and the form reads its own when it opens.
    @ObservationIgnored
    let candidateSignalsSubject = PassthroughSubject<
        CandidateSignalsEvent, Never
    >()

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
        let deliveredSession = incoming.identityPickSession.flatMap {
            session -> CandidateIdentityPickSession? in
            guard detail.row.picked == session.pick else { return nil }
            session.recordDetailDelivery()
            return session
        }
        selectedCandidates[key] = incoming
        if let deliveredSession {
            finishIdentityPickIfConfirmed(
                key: key,
                session: deliveredSession
            )
        }
    }

    /// Start one identity-pick session before its bridge command is dispatched,
    /// so an immediate candidate-detail delivery cannot outrun registration.
    /// Replacing a session drops its task owner and cancels the older command.
    func beginIdentityPick(
        key: String,
        pick: BridgeIdentityPick,
        onConfirmed: (@Sendable () -> Void)? = nil
    ) -> CandidateIdentityPickSession? {
        guard candidate(forKey: key) != nil else { return nil }
        let session = CandidateIdentityPickSession(
            pick: pick,
            onConfirmed: onConfirmed
        )
        mutateCandidate(forKey: key) { candidate in
            candidate.error = nil
            candidate.identityPickSession = session
        }
        return session
    }

    /// Record that the bridge command returned successfully. A release choice
    /// still remains pending until its exact picked detail has also landed.
    func identityPickCommandSucceeded(
        key: String,
        session: CandidateIdentityPickSession
    ) {
        guard candidate(forKey: key)?.identityPickSession === session else {
            return
        }
        session.recordCommandSuccess()
        finishIdentityPickIfConfirmed(key: key, session: session)
    }

    /// End only the session that raised this failure. A replacement choice may
    /// already own the candidate by the time an older command returns.
    func identityPickFailed(
        key: String,
        session: CandidateIdentityPickSession,
        error: String?
    ) {
        guard candidate(forKey: key)?.identityPickSession === session else {
            return
        }
        mutateCandidate(forKey: key) { candidate in
            if let error {
                candidate.error = error
            }
            candidate.presentedIdentity = candidate.identity
            candidate.identityPickSession = nil
        }
    }

    private func finishIdentityPickIfConfirmed(
        key: String,
        session: CandidateIdentityPickSession
    ) {
        guard session.isConfirmed,
            candidate(forKey: key)?.identityPickSession === session
        else { return }
        let confirmation = session.takeConfirmation()
        mutateCandidate(forKey: key) { candidate in
            guard candidate.identityPickSession === session else { return }
            candidate.identityPickSession = nil
        }
        confirmation?()
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

    func presentIdentity(_ identity: ImportIdentity, forKey key: String) {
        mutateCandidate(forKey: key) {
            $0.presentedIdentity = identity
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
