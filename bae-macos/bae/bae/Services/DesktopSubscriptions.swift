import BaeKit
import Foundation

private final class CandidateRuntimeSink: CandidateRuntimeCallback,
    @unchecked Sendable
{
    private let apply:
        @MainActor @Sendable ([BridgeCandidateRuntimeChange]) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable ([BridgeCandidateRuntimeChange])
            -> Void
    ) {
        self.apply = apply
    }

    func onChanges(changes: [BridgeCandidateRuntimeChange]) {
        Task { @MainActor in apply(changes) }
    }
}

private final class FolderScansSink: FolderScansCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeFolderScans) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeFolderScans) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeFolderScans) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

/// The read behind the one selected import candidate, whose key moves in place
/// as the selection moves. Several selected candidates, or none, read nothing
/// here: the pane several open reads core's selection summary instead.
@MainActor
final class ImportSelectionObservation {
    private let open: () -> DetailQuery<BridgeImportCandidateDetail>
    private let importStore: ImportStore
    private let uiStore: UiStore
    private var reader: DetailReader<BridgeImportCandidateDetail>?

    init(
        open: @escaping () -> DetailQuery<BridgeImportCandidateDetail>,
        importStore: ImportStore,
        uiStore: UiStore
    ) {
        self.open = open
        self.importStore = importStore
        self.uiStore = uiStore
    }

    convenience init(
        appHandle: AppHandle,
        importStore: ImportStore,
        uiStore: UiStore
    ) {
        self.init(
            open: {
                let subscription = appHandle.subscribeImportCandidate()
                return DetailQuery(
                    setId: { try subscription.setId(id: $0) },
                    next: {
                        let snapshot = try await subscription.next()
                        return DetailDelivery(
                            id: snapshot.id,
                            value: snapshot.value
                        )
                    },
                    cancel: { try? await subscription.cancel() }
                )
            },
            importStore: importStore,
            uiStore: uiStore
        )
    }

    /// Read `single`, the one selected candidate, or nothing.
    func selectionChanged(single: String?) {
        guard single != reader?.id else { return }
        if let previous = reader?.id {
            importStore.selectedCandidates.removeValue(forKey: previous)
        }
        guard let single else {
            reader?.close()
            reader = nil
            return
        }
        let reader = reader ?? makeReader()
        self.reader = reader
        reader.show(single)
    }

    private func makeReader() -> DetailReader<BridgeImportCandidateDetail> {
        DetailReader(
            open: open,
            onValue: { [weak self] key, detail in
                self?.deliver(detail, key: key)
            },
            onError: { [weak self] _, error in
                self?.uiStore.showError(error)
            }
        )
    }

    private func deliver(_ detail: BridgeImportCandidateDetail?, key: String) {
        guard let detail else {
            // The key names no scanned folder any more: a pick made on it has
            // nothing left to claim, and core took it out of the selection in
            // the write that removed it.
            importStore.cancelMetadataApplication(forKey: key)
            return
        }
        importStore.applyCandidateDetail(key: key, detail: detail)
    }
}

/// The read behind the one selected folder that failed validation, whose key
/// moves in place as the selection moves. Any other selection reads nothing
/// here.
@MainActor
final class InvalidCandidateSelectionObservation {
    private let open: () -> DetailQuery<BridgeInvalidCandidateDetail>
    private let importStore: ImportStore
    private let uiStore: UiStore
    private var reader: DetailReader<BridgeInvalidCandidateDetail>?

    init(
        open: @escaping () -> DetailQuery<BridgeInvalidCandidateDetail>,
        importStore: ImportStore,
        uiStore: UiStore
    ) {
        self.open = open
        self.importStore = importStore
        self.uiStore = uiStore
    }

    convenience init(
        appHandle: AppHandle,
        importStore: ImportStore,
        uiStore: UiStore
    ) {
        self.init(
            open: {
                let subscription = appHandle.subscribeInvalidCandidate()
                return DetailQuery(
                    setId: { try subscription.setId(id: $0) },
                    next: {
                        let snapshot = try await subscription.next()
                        return DetailDelivery(
                            id: snapshot.id,
                            value: snapshot.value
                        )
                    },
                    cancel: { try? await subscription.cancel() }
                )
            },
            importStore: importStore,
            uiStore: uiStore
        )
    }

    /// Read `key`, the one selected folder that failed validation, or
    /// nothing.
    func selectionChanged(key: String?) {
        guard key != reader?.id else { return }
        if let previous = reader?.id {
            importStore.selectedInvalidCandidates.removeValue(forKey: previous)
        }
        guard let key else {
            reader?.close()
            reader = nil
            return
        }
        let reader =
            reader
            ?? DetailReader(
                open: open,
                onValue: { [weak self] key, detail in
                    // A key whose folder no longer fails reads nothing; the
                    // selection then reads it as the candidate it became.
                    self?.importStore.selectedInvalidCandidates[key] = detail
                },
                onError: { [weak self] _, error in
                    self?.uiStore.showError(error)
                }
            )
        self.reader = reader
        reader.show(key)
    }
}

@MainActor
final class DesktopSubscriptions {
    /// The import sidebar's paged list, installed in the view environment.
    let importList: ImportListSlot

    private let appHandle: AppHandle
    private let importStore: ImportStore
    private let outputStore: OutputStore
    private let outboxStore: OutboxStore
    private let downloadStore: DownloadStore
    private let uiStore: UiStore
    /// The import list's selection, installed in the view environment.
    let importSelection: ImportSelection
    private var subscriptions: [LiveSubscription] = []
    /// The fast streams' feeds, which stop with these subscriptions.
    private var feeds: [LatestFeed] = []

    init(
        appHandle: AppHandle,
        importStore: ImportStore,
        outputStore: OutputStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        uiStore: UiStore
    ) {
        self.appHandle = appHandle
        self.importStore = importStore
        self.outputStore = outputStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.uiStore = uiStore
        // The list's selection sink is the observations' one owner: it lives
        // as long as the list slot that calls it.
        let selectionObservation = ImportSelectionObservation(
            appHandle: appHandle,
            importStore: importStore,
            uiStore: uiStore
        )
        let invalidSelectionObservation = InvalidCandidateSelectionObservation(
            appHandle: appHandle,
            importStore: importStore,
            uiStore: uiStore
        )
        let selectionOwner = ImportListSelectionOwner()
        let importSelection = ImportSelection(
            operations: .live(list: selectionOwner)
        )
        self.importSelection = importSelection
        let applySelection = Self.selectionSink(
            importStore: importStore,
            importSelection: importSelection,
            observations: (selectionObservation, invalidSelectionObservation)
        )
        // A watched folder that could not be read. Wired before anything can
        // deliver the folder scans, and fed from their live query rather than
        // a transient event, so a scan that failed while the app was still
        // starting up is raised on the first delivery instead of being
        // published to nobody.
        importStore.onScanFailure = Self.scanFailureAlert(on: uiStore)
        importList = ImportListSlot(
            importStore: importStore,
            uiStore: uiStore,
            selection: importSelection,
            makeSource: { view in
                let list = try await appHandle.subscribeImportList(view: view)
                selectionOwner.own(list)
                return ImportListPageSource(
                    subscription: list,
                    onSummary: { summary in
                        importStore.applySummary(summary)
                    },
                    onPendingFilterEntries: { entries in
                        importStore.applyPendingFilterEntries(entries)
                    },
                    onSelection: applySelection
                )
                .pages
            }
        )
    }

    /// Where the list's selection goes with each of its values: the
    /// selection the panes read, the revision the rows reflect, and the read
    /// behind the one selected row.
    private static func selectionSink(
        importStore: ImportStore,
        importSelection: ImportSelection,
        observations: (
            ImportSelectionObservation, InvalidCandidateSelectionObservation
        )
    ) -> @MainActor @Sendable (BridgeSelectionSummary, UInt64) -> Void {
        { summary, revision in
            importStore.applySelectionRevision(revision)
            importSelection.apply(summary)
            observations.0.selectionChanged(
                single: summary.single?.candidateKey
            )
            observations.1.selectionChanged(
                key: summary.single?.invalidCandidateKey
            )
        }
    }

    /// Raise a watched folder that could not be read on the app's alert,
    /// with the untranslated fault as its detail.
    private static func scanFailureAlert(
        on uiStore: UiStore
    ) -> (_ watchedFolderPath: String, _ detail: String) -> Void {
        { [uiStore] watchedFolderPath, detail in
            uiStore.showError(
                DisplayError(
                    line: String(
                        format: NSLocalizedString(
                            "ui.import.folder.scan_failed",
                            tableName: "Core",
                            bundle: .main,
                            comment: ""
                        ),
                        watchedFolderPath
                    ),
                    detail: detail
                )
            )
        }
    }

    func start() {
        precondition(subscriptions.isEmpty)
        feeds = [
            outputStore.percents.feed(from: appHandle),
            outboxStore.fileShares.feedUploads(from: appHandle),
            downloadStore.fileShares.feedPins(from: appHandle),
        ]
        subscriptions = [
            appHandle.subscribeCandidateRuntime(
                callback: CandidateRuntimeSink { [importStore] changes in
                    importStore.candidateRuntimeSubject.send(changes)
                }
            ),
            appHandle.subscribeFolderScans(
                callback: FolderScansSink(
                    apply: { [importStore] scans in
                        importStore.applyFolderScans(scans)
                    },
                    fail: { [uiStore] error in uiStore.showError(error) }
                )
            ),
        ]
        importList.startLoad()
    }
}
