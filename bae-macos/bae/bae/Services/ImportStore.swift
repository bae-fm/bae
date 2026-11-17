import BaeKit
import OrderedCollections
import SwiftUI
import os.log

private let importStoreLogger = Logger.bae("ImportStore")

/// Session state for the import flow. Mixed-writer: core drives the list, the
/// per-candidate reads and shot state through value subscriptions, while
/// views drive user-set fields (presentation, selected cover) via
/// `mutateCandidate(forKey:_:)`. The single-writer rule applies per field, not
/// per store.
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

    /// The folders being scanned, each with how many folders its scan has
    /// stored, from their own subscription: a scan moves its count with
    /// every folder it walks, which moves nothing the folder scans show.
    /// Empty until the first delivery.
    private(set) var scanningFolders: [BridgeScanningFolder] = []

    /// The scan alerts this store raised whose write that marks them shown
    /// has not finished, each with the task that writes the mark. A delivery
    /// made before the write commits still lists the alert, and the store
    /// does not raise it again; once the write finishes, the folder scans say
    /// whether they still list it.
    @ObservationIgnored
    private var scanAlertMarks: [BridgeFolderScanAlert: Task<Void, Never>] =
        [:]

    /// The store calls this for each watched folder whose scan failed or
    /// that is not watched, while the folder scans list an alert for it.
    /// Whoever owns an error surface sets it: this store has none.
    ///
    /// The alert arrives on the folder scans rather than as a transient
    /// event because core stores the problem and which alerts the app
    /// raised, so core delivers it whenever the UI subscribes rather than
    /// only to whoever was already listening. A scan that failed during
    /// launch, before the store subscribed, is in the first delivery.
    @ObservationIgnored
    var onScanFailure: ((BridgeFolderScanAlert) -> Void)?

    /// Records in core that the app raised an alert, so the folder scans
    /// stop listing it. Whoever sets `onScanFailure` sets this too.
    @ObservationIgnored
    var markScanAlertShown: ((BridgeFolderScanAlert) async throws -> Void)?

    /// Take a delivered summary, unless it is the one already held.
    func applySummary(_ next: BridgeImportQueueSummary) {
        guard next != summary else { return }
        summary = next
    }

    /// Take the delivered scanning folders.
    func applyScanningFolders(_ next: [BridgeScanningFolder]) {
        scanningFolders = next
    }

    /// Take delivered folder scans, and raise each alert they list that this
    /// store is not already marking shown.
    @MainActor
    func applyFolderScans(_ next: BridgeFolderScans) {
        folderScans = next
        for alert in next.alerts where scanAlertMarks[alert] == nil {
            raiseScanAlert(alert)
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
    /// Where a folder candidate's session writes go: core stores them with
    /// the candidate and the next detail carries them back. A re-identify
    /// session has no stored candidate, so its session stays in memory here.
    /// Inert until the app wires the importer in.
    var sessionWriter = CandidateSessionWriter.inert

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
        let incoming = Candidate(detail: detail)
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
    /// the draft. Written through core rather than in memory, so the candidate
    /// is where the pick left it whether or not anyone was watching.
    func metadataApplicationSucceeded(
        key: Data,
        session: CandidateMetadataApplicationSession
    ) {
        guard metadataApplicationSession(forKey: key) === session else {
            return
        }
        // Core moved the pane to the draft the pick landed on.
        picks.removeValue(forKey: key)
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

    /// Move a folder candidate's pane as the person asked. Core moves it by
    /// the rule it moves every pane by, and the next detail carries it back.
    /// A re-identify session has no pane to move: the release editor opens
    /// and leaves its search page itself.
    func movePane(_ paneMove: BridgePaneMove, forKey key: Data) {
        guard reIdentifyCandidates[key] == nil else { return }
        let writer = sessionWriter
        Task {
            do {
                try await writer.movePane(key, paneMove)
            }
            catch {
                importStoreLogger.error(
                    "pane move for \(bridgeCandidateKeyText(candidateKey: key)) failed: \(String(describing: error))"
                )
                await writer.reportFailure(error)
            }
        }
    }

    /// The typed-search form as the person left it.
    func commitSearchForm(_ form: CandidateSearchState, forKey key: Data) {
        updateSession(
            forKey: key,
            memory: { $0.search = form },
            stored: { writer in try await writer.setSearchForm(key, form.bridge)
            }
        )
    }

    /// Tell the person about a failure no pane states: a pane command whose
    /// failure core could not store, or a read the pane depends on.
    @MainActor
    func reportFailure(_ error: Error) {
        sessionWriter.reportFailure(error)
    }

    /// One session change, routed by what the key names: a re-identify
    /// session is edited in memory, a folder candidate's is written through
    /// core, which delivers it back on the candidate's next detail. A write
    /// that fails is told to the person rather than left as a pane state the
    /// store never saw.
    private func updateSession(
        forKey key: Data,
        memory: @escaping (inout CandidateSessionState) -> Void,
        stored write:
            @escaping @Sendable (CandidateSessionWriter) async throws -> Void
    ) {
        if reIdentifyCandidates[key] != nil {
            mutateReIdentifyCandidate(key: key) { memory(&$0.session) }
            return
        }
        let writer = sessionWriter
        Task {
            do {
                try await write(writer)
            }
            catch {
                importStoreLogger.error(
                    "session write for \(bridgeCandidateKeyText(candidateKey: key)) failed: \(String(describing: error))"
                )
                await writer.reportFailure(error)
            }
        }
    }
}

extension ImportStore {
    /// Raise `alert`, and mark it shown in core. Until that write finishes,
    /// the store does not raise the alert again for a delivery that still
    /// lists it; a mark that fails leaves the alert in the list, so the
    /// store raises it again on the next delivery.
    @MainActor
    private func raiseScanAlert(_ alert: BridgeFolderScanAlert) {
        onScanFailure?(alert)
        let mark = markScanAlertShown
        scanAlertMarks[alert] = Task { @MainActor [weak self] in
            do {
                try await mark?(alert)
            }
            catch {
                importStoreLogger.error(
                    "core did not mark the scan alert for \(alert.folder.name) shown: \(String(describing: error))"
                )
                self?.reportFailure(error)
            }
            self?.scanAlertMarks[alert] = nil
        }
    }

    /// Wait until every scan alert mark this store started has returned.
    @MainActor
    func scanAlertMarksReturned() async {
        while let mark = scanAlertMarks.values.first {
            await mark.value
        }
    }

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
