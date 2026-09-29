import BaeKit
import Foundation

private final class OutputValueSink: OutputCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeOutputSnapshot) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeOutputSnapshot) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeOutputSnapshot) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

private final class CandidateRuntimeSink: CandidateRuntimeCallback,
    @unchecked Sendable
{
    private let apply:
        @MainActor @Sendable (BridgeCandidateRuntimeChange) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeCandidateRuntimeChange)
            -> Void
    ) {
        self.apply = apply
    }

    func onChange(change: BridgeCandidateRuntimeChange) {
        Task { @MainActor in apply(change) }
    }
}

private final class ImportSelectionSink: ImportSelectionCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeSelectionSummary) -> Void

    init(apply: @escaping @MainActor @Sendable (BridgeSelectionSummary) -> Void)
    {
        self.apply = apply
    }

    func onValue(value: BridgeSelectionSummary) {
        Task { @MainActor in apply(value) }
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

@MainActor
final class DesktopSubscriptions {
    /// The import sidebar's paged list, installed in the view environment.
    let importList: ImportListSlot

    private let appHandle: AppHandle
    private let importStore: ImportStore
    private let outputStore: OutputStore
    private let uiStore: UiStore
    /// The import list's selection, installed in the view environment.
    let importSelection: ImportSelection
    private let selectionObservation: ImportSelectionObservation
    private var subscriptions: [LiveSubscription] = []

    init(
        appHandle: AppHandle,
        importStore: ImportStore,
        outputStore: OutputStore,
        uiStore: UiStore
    ) {
        self.appHandle = appHandle
        self.importStore = importStore
        self.outputStore = outputStore
        self.uiStore = uiStore
        selectionObservation = ImportSelectionObservation(
            appHandle: appHandle,
            importStore: importStore,
            uiStore: uiStore
        )
        let importSelection = ImportSelection(
            operations: .live(handle: appHandle)
        )
        self.importSelection = importSelection
        // A watched folder that could not be read. Wired before anything can
        // deliver a summary, and fed from the list's live query rather than a
        // transient event, so a scan that failed while the app was still
        // starting up is raised on the first delivery instead of being
        // published to nobody.
        importStore.onScanFailure = { [uiStore] watchedFolderPath, detail in
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
        importList = ImportListSlot(
            importStore: importStore,
            uiStore: uiStore,
            selection: importSelection,
            makeSource: { view in
                ImportListPageSource(
                    subscription: appHandle.subscribeImportList(view: view),
                    onSummary: { summary in
                        importStore.applySummary(summary)
                    },
                    onSelectionRevision: { revision in
                        importStore.applySelectionRevision(revision)
                    }
                )
                .pages
            },
            locateCandidate: { view, key in
                try await appHandle.locateImportCandidate(
                    view: view,
                    candidateKey: key
                )
            },
            firstIdentifyingCandidate: { view in
                try await appHandle.firstIdentifyingCandidate(view: view)
            }
        )
    }

    func start() {
        precondition(subscriptions.isEmpty)
        subscriptions = [
            appHandle.subscribeOutputs(
                callback: OutputValueSink(
                    apply: { [outputStore] value in
                        outputStore.applySnapshot(value)
                    },
                    fail: { [uiStore] error in uiStore.showError(error) }
                )
            ),
            appHandle.subscribeCandidateRuntime(
                callback: CandidateRuntimeSink { [importStore] change in
                    importStore.candidateRuntimeSubject.send(change)
                }
            ),
            appHandle.subscribeImportSelection(
                callback: ImportSelectionSink {
                    [importSelection, selectionObservation] summary in
                    importSelection.apply(summary)
                    selectionObservation.selectionChanged(
                        single: summary.single
                    )
                }
            ),
        ]
        importList.startLoad()
    }
}
