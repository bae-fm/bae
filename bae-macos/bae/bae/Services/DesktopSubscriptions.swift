import BaeKit
import Foundation

private final class OutputValueSink: OutputCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeOutputSnapshot) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeOutputSnapshot) -> Void
    ) {
        self.apply = apply
    }

    func onValue(value: BridgeOutputSnapshot) {
        Task { @MainActor in apply(value) }
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

/// The reads behind the selected import candidates: one read per selected
/// key, each a read whose key moves in place. A selection that moves to other
/// candidates moves the reads it has instead of closing them and opening new
/// ones; only a selection that grows opens more, and one that shrinks closes
/// what it no longer needs. Each candidate keeps a read of its own because
/// each changes on its own. A read that says the folder is gone drops the key
/// from the selection, which is what clears a row the scan removed.
@MainActor
final class ImportSelectionObservations {
    private let open: () -> DetailQuery<BridgeImportCandidateDetail>
    private let importStore: ImportStore
    private let uiStore: UiStore
    private var readers: [DetailReader<BridgeImportCandidateDetail>] = []

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

    func selectionChanged(_ keys: Set<String>) {
        var free: [DetailReader<BridgeImportCandidateDetail>] = []
        var shown: Set<String> = []
        for reader in readers {
            if let key = reader.id, keys.contains(key) {
                shown.insert(key)
                continue
            }
            if let key = reader.id {
                importStore.selectedCandidates.removeValue(forKey: key)
            }
            free.append(reader)
        }
        for key in keys.subtracting(shown).sorted() {
            if let reader = free.popLast() {
                reader.show(key)
            }
            else {
                let reader = makeReader()
                readers.append(reader)
                reader.show(key)
            }
        }
        for reader in free {
            reader.close()
            readers.removeAll { $0 === reader }
        }
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
            // The key names no scanned folder any more, so nothing can be done
            // with it: a pick made on it has nothing left to claim, and the
            // key leaves the selection, which frees this read.
            importStore.cancelMetadataApplication(forKey: key)
            uiStore.removeFolderCandidateSelection([key])
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
    private let selection: ImportSelectionObservations
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
        selection = ImportSelectionObservations(
            appHandle: appHandle,
            importStore: importStore,
            uiStore: uiStore
        )
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
            makeSource: { view in
                ImportListPageSource(
                    subscription: appHandle.subscribeImportList(view: view),
                    onSummary: { summary in
                        importStore.applySummary(summary)
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
                callback: OutputValueSink { [outputStore] value in
                    outputStore.applySnapshot(value)
                }
            ),
            appHandle.subscribeCandidateRuntime(
                callback: CandidateRuntimeSink { [importStore] change in
                    importStore.candidateRuntimeSubject.send(change)
                }
            ),
        ]
        uiStore.onFolderCandidateSelectionChanged = { [selection] keys in
            selection.selectionChanged(keys)
        }
        importList.startLoad()
    }
}
