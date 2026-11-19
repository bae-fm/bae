import BaeKit
import OrderedCollections
import SwiftUI
import os.log

private let importStoreLogger = Logger.bae("ImportStore")

/// Session state for the import flow. Mixed-writer: core drives the list, the
/// per-candidate reads and shot state through value subscriptions, while
/// views drive the pane's own state (where it stands, the typed search form)
/// via `mutateCandidate(forKey:_:)`. The single-writer rule applies per field,
/// not per store.
///
/// `ImportListSlot` holds the list's rows and their order:
/// `selectedCandidates` holds the whole folder for each row the user
/// selected. Keys are unique across the selected folders and the re-identify
/// sessions (a folder path or `reidentify:{releaseId}`), so the cross-type
/// helpers below look a candidate up by key without knowing its source.
@Observable
class ImportStore {
    /// An empty store, made on the main actor, where the streams it holds
    /// are made and read.
    @MainActor
    init() {}

    /// Everything the chrome around the list shows: the watched folders and
    /// whether Found has groups to fold. `nil` until
    /// the first delivery: a queue with no watched folders is a different
    /// thing from a queue not read yet, and each surface says which.
    ///
    /// Written through `applySummary`, which drops a delivery that says the
    /// same thing as the one before it: every verdict the sweep commits
    /// re-delivers this whole value, and an equal write would still invalidate
    /// everything reading it.
    private(set) var summary: BridgeImportQueueSummary?

    /// Where each watched folder's scan stands, from the folder scans' own
    /// subscription: a scan moves its found count with every folder it walks,
    /// which moves nothing the list shows. The value is `nil` until core
    /// sends the first folder scans. Core's query sends a value only when it
    /// changed, so `applyFolderScans` writes each delivery as it arrives.
    private(set) var folderScans: BridgeFolderScans?

    /// The folders bae is scanning, each with how many folders its scan has
    /// stored, from their own subscription: each folder a scan walks raises
    /// its count, and that change leaves the folder scans as they are. The
    /// list is empty until the first delivery.
    private(set) var scanningFolders: [BridgeScanningFolder] = []

    /// Every scan alert this store raised while the app runs. The folder
    /// scans' query takes them in its request and lists no alert for them.
    /// The store sets them on the query before it pulls the next delivery,
    /// so no delivery lists an alert the store raised.
    @ObservationIgnored
    private var raisedScanAlerts: Set<BridgeFolderScanAlert> = []

    /// The store calls this for each watched folder whose scan failed or
    /// that is not watched, while the folder scans list an alert for it.
    /// Whoever owns an error surface sets it: this store has none.
    ///
    /// The alert arrives on the folder scans rather than as a transient
    /// event because core stores the problem, so core delivers it whenever
    /// the UI subscribes rather than only to whoever was already listening.
    /// A scan that failed during launch, before the store subscribed, is in
    /// the first delivery.
    @ObservationIgnored
    var onScanFailure: ((BridgeFolderScanAlert) -> Void)?

    /// Hands the folder scans' query every alert the store raised, so the
    /// folder scans stop listing them. Whoever subscribes to the folder scans
    /// sets it.
    @ObservationIgnored
    var setRaisedScanAlerts: (([BridgeFolderScanAlert]) throws -> Void)?

    /// Take a delivered summary, unless it is the one already held.
    func applySummary(_ next: BridgeImportQueueSummary) {
        guard next != summary else { return }
        summary = next
    }

    /// Take the delivered scanning folders.
    func applyScanningFolders(_ next: [BridgeScanningFolder]) {
        scanningFolders = next
    }

    /// Take delivered folder scans, raise each alert they list, and hand the
    /// query every alert raised so far. Handing it the alerts it already has
    /// reads nothing again.
    @MainActor
    func applyFolderScans(_ next: BridgeFolderScans) {
        folderScans = next
        for alert in next.alerts {
            onScanFailure?(alert)
            raisedScanAlerts.insert(alert)
        }
        do {
            try setRaisedScanAlerts?(Array(raisedScanAlerts))
        }
        catch {
            if case BridgeError.Cancelled = error {
                importStoreLogger.debug(
                    "the folder scans took no raised alerts because their library closed: \(String(reflecting: error))"
                )
                return
            }
            importStoreLogger.error(
                "the folder scans did not take the raised alerts: \(String(describing: error))"
            )
            reportFailure(error)
        }
    }

    /// Every entry of Found's filter, in core's order, with how many of
    /// Found's rows it holds, as the list's last delivery counted them. Empty
    /// before the first delivery, when the list has nothing to count.
    ///
    /// Written through `applyPendingFilterEntries`, which drops a delivery
    /// that counts the same as the one before it.
    private(set) var pendingFilterEntries: [BridgePendingFilterEntry] = []

    /// Take a delivery's filter entries, unless they count what the held ones
    /// do.
    func applyPendingFilterEntries(_ next: [BridgePendingFilterEntry]) {
        guard next != pendingFilterEntries else { return }
        pendingFilterEntries = next
    }

    /// The selected rows' folders, read by key. A selection opens one
    /// subscription per key; the key leaves when its read says the folder is
    /// gone.
    var selectedCandidates: OrderedDictionary<Data, Candidate> = [:]
    /// The selected folder that failed validation, as the pane looking into
    /// it reads it, by key; the key leaves when the selection moves off it or
    /// its read says the folder no longer fails.
    var selectedInvalidCandidates: [Data: BridgeInvalidCandidateDetail] = [:]
    /// Where a failure no pane states goes: a pane command whose failure
    /// core could not store, or a read the pane depends on. Tells no one
    /// until the app wires in its error surface.
    @ObservationIgnored
    var onFailure: @MainActor @Sendable (Error) -> Void = { _ in }

    /// Library releases' catalog searches — one per release editor that
    /// opened its search.
    /// Keyed by `reidentify:{releaseId}` so identify events route the same
    /// way folder events do. The sheet inserts on open and removes on dismiss.
    var reIdentifyCandidates: OrderedDictionary<Data, Candidate> = [:]

    /// Whether bae watches any folder for imports; `nil` until the first
    /// summary arrives.
    var hasWatchedFolders: Bool? {
        summary?.hasWatchedFolders
    }

    /// How far the identifications running right now have got — how many
    /// have ended, out of how many there are, whoever started them — for the
    /// filter row's ring and the progress line it opens. `nil` before the
    /// counts' live query has delivered; `(0, 0)` once nothing is running.
    var identificationProgress: (identified: UInt32, total: UInt32)?

    /// How many imports are waiting or running, whoever started them, for the
    /// filter row's import indicator. Zero shows nothing.
    var importsInFlight: UInt32 = 0

    var previewState: BridgePreviewState = .idle

    /// Where the import tab's shot is in its clip, while one is loaded,
    /// for the native bar that reads it each display frame.
    @ObservationIgnored
    let previewPosition = LatestStream<BridgePreviewPosition>
        .previewPosition()

    /// How far through its step each running import is, for the native bars
    /// that read it each display frame.
    @ObservationIgnored
    let importPercents = LatestStream<UInt32>.importPercents()

    /// Look up a candidate by key across both source dicts. Used when the
    /// caller doesn't know (or doesn't care about) the candidate's source —
    /// the shared search/confirmation flow.
    func candidate(forKey key: Data) -> Candidate? {
        selectedCandidates[key] ?? reIdentifyCandidates[key]
    }

    /// The effective cover resolved by core for this row.
    func sidebarCover(for row: BridgeTriageRow) -> ImageContent? {
        row.cover.map(ImageContent.init(bridge:))
    }

    // MARK: - Per-key reads

    /// One candidate's metadata pick: what it is doing, and the audio it was
    /// made about.
    private struct Pick {
        /// The identity of the audio the folder held when the pick was made.
        /// Audio that changes under it leaves the pick a claim about a folder
        /// that is no longer there.
        let audioIdentity: String
        var state: CandidateMetadataApplication
    }

    /// What each candidate's metadata pick is doing, under that candidate's
    /// key. Picking is a decision about one folder, so it is held here rather
    /// than on the selected candidate: looking at another candidate — or at
    /// none — neither cancels the read nor loses the failure it left on a
    /// pressing. An entry ends when its own task ends, and is cancelled only
    /// when the candidate stops being a scanned folder or its audio changes
    /// underneath it.
    private var picks: [Data: Pick] = [:]

    /// One selected candidate, as its own read describes it, keeping whatever
    /// work the session has done on it.
    func applyCandidateDetail(
        key: Data,
        detail: BridgeImportCandidateDetail
    ) {
        var incoming = Candidate(detail: detail)
        // The pane's own state stays while the pane shows this candidate.
        incoming.pane = selectedCandidates[key]?.pane ?? CandidatePaneState()
        if let pick = picks[key],
            pick.audioIdentity != incoming.files.fileMetadataIdentity
        {
            cancelMetadataApplication(forKey: key)
        }
        selectedCandidates[key] = incoming
    }

    /// The read one candidate's pick has in flight.
    func metadataApplicationSession(
        forKey key: Data
    ) -> CandidateMetadataApplicationSession? {
        guard case .applying(let session) = picks[key]?.state
        else { return nil }
        return session
    }

    /// Whether a metadata source is being read into this candidate's draft.
    func isApplyingMetadata(forKey key: Data) -> Bool {
        metadataApplicationSession(forKey: key) != nil
    }

    /// The pick this candidate is waiting on, for the row it was made on to
    /// name what the read waits on.
    func loadingPick(forKey key: Data) -> LoadingPick? {
        guard let session = metadataApplicationSession(forKey: key),
            case .pick(let link) = session.application
        else { return nil }
        return LoadingPick(pressing: link.record, apply: session.apply)
    }

    /// How this candidate's last pick failed, on the pressing it was about.
    func releaseSelectionFailure(
        forKey key: Data
    ) -> ReleaseSelectionFailure? {
        guard case .failed(let failure) = picks[key]?.state
        else { return nil }
        return failure
    }

    /// Start one pick, before its bridge command is dispatched. Replacing a
    /// pick drops the older session, and dropping that session cancels the
    /// read it had running.
    func beginMetadataApplication(
        key: Data,
        application: MetadataApplication,
        apply: (any CatalogApplySource)?
    ) -> CandidateMetadataApplicationSession? {
        guard let candidate = candidate(forKey: key) else { return nil }
        let session = CandidateMetadataApplicationSession(
            application: application,
            apply: apply
        )
        picks[key] = Pick(
            audioIdentity: candidate.files.fileMetadataIdentity,
            state: .applying(session)
        )
        return session
    }

    /// The pick landed: core holds the draft it read, so the pane goes back to
    /// the draft.
    func metadataApplicationSucceeded(
        key: Data,
        session: CandidateMetadataApplicationSession
    ) {
        guard metadataApplicationSession(forKey: key) === session else {
            return
        }
        picks.removeValue(forKey: key)
        movePane(.picked, forKey: key)
    }

    /// End only the session that raised this failure. A replacement choice may
    /// already own the candidate by the time an older command returns. A
    /// catalog release that failed to load says so on its own row; any other
    /// failure is the pane's to state, or was told already, and the pick ends.
    func metadataApplicationFailed(
        key: Data,
        session: CandidateMetadataApplicationSession,
        error: DisplayError?
    ) {
        guard metadataApplicationSession(forKey: key) === session else {
            return
        }
        if let error,
            case .pick(let link) = session.application
        {
            picks[key]?.state = .failed(
                ReleaseSelectionFailure(
                    release: link.record,
                    error: error
                )
            )
            return
        }
        picks.removeValue(forKey: key)
    }

    /// Drop this candidate's pick, cancelling the read it has in flight: the
    /// folder it claimed is gone, or is no longer the folder it was made
    /// about.
    func cancelMetadataApplication(forKey key: Data) {
        picks.removeValue(forKey: key)
    }
}

extension ImportStore {
    private func mutateSelectedCandidate(
        key: Data,
        _ mutate: (inout Candidate) -> Void
    ) {
        if var candidate = selectedCandidates[key] {
            mutate(&candidate)
            selectedCandidates[key] = candidate
        }
    }

    private func mutateReIdentifyCandidate(
        key: Data,
        _ mutate: (inout Candidate) -> Void
    ) {
        if var candidate = reIdentifyCandidates[key] {
            mutate(&candidate)
            reIdentifyCandidates[key] = candidate
        }
    }

    func mutateCandidate(
        forKey key: Data,
        _ mutate: (inout Candidate) -> Void
    ) {
        if reIdentifyCandidates[key] != nil {
            mutateReIdentifyCandidate(key: key, mutate)
        }
        else {
            mutateSelectedCandidate(key: key, mutate)
        }
    }

    /// Move the candidate's pane as the person, or a command they ran,
    /// asked. The pane holds where it stands; nothing is written.
    func movePane(_ paneMove: PaneMove, forKey key: Data) {
        mutateCandidate(forKey: key) { $0.pane.apply(paneMove) }
    }

    /// The typed-search form as the person left it.
    func commitSearchForm(_ form: CandidateSearchState, forKey key: Data) {
        mutateCandidate(forKey: key) { $0.pane.search = form }
    }

    /// Tell the person about a failure no pane states: a pane command whose
    /// failure core could not store, or a read the pane depends on.
    @MainActor
    func reportFailure(_ error: Error) {
        onFailure(error)
    }
}

extension ImportStore {
    /// Take delivered import counts, writing each only when it moved so its
    /// readers redraw only then.
    func applyImportCounts(_ counts: BridgeImportCounts) {
        let progress = (identified: counts.identified, total: counts.admitted)
        if identificationProgress.map({ $0 != progress }) ?? true {
            identificationProgress = progress
        }
        if importsInFlight != counts.importsInFlight {
            importsInFlight = counts.importsInFlight
        }
    }
}
