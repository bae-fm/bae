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

private final class ImportCandidateSink: ImportCandidateCallback,
    @unchecked Sendable
{
    private let apply:
        @MainActor @Sendable (BridgeImportCandidateDetail?) -> Void
    private let fail: @MainActor @Sendable (BridgeError) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeImportCandidateDetail?)
            -> Void,
        fail: @escaping @MainActor @Sendable (BridgeError) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeImportCandidateDetail?) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

/// The reads behind the selected import candidates: one per selected key,
/// opened when the key is selected and closed when it leaves the selection.
/// A read that says the folder is gone drops the key from the selection, which
/// is what clears a row the scan removed.
@MainActor
private final class ImportSelectionObservations {
    private struct Observation {
        let identity: UUID
        let subscription: LiveSubscription
    }

    private let appHandle: AppHandle
    private let importStore: ImportStore
    private let uiStore: UiStore
    private let importer: Importer
    private var observations: [String: Observation] = [:]

    init(
        appHandle: AppHandle,
        importStore: ImportStore,
        uiStore: UiStore,
        importer: Importer
    ) {
        self.appHandle = appHandle
        self.importStore = importStore
        self.uiStore = uiStore
        self.importer = importer
    }

    func selectionChanged(_ keys: Set<String>) {
        for key in observations.keys where !keys.contains(key) {
            observations.removeValue(forKey: key)?.subscription.cancel()
            importStore.selectedCandidates.removeValue(forKey: key)
        }
        for key in keys where observations[key] == nil {
            observe(key)
        }
    }

    private func observe(_ key: String) {
        let identity = UUID()
        let subscription = appHandle.subscribeImportCandidate(
            candidateKey: key,
            callback: ImportCandidateSink(
                apply: { [weak self] detail in
                    self?.deliver(detail, key: key, identity: identity)
                },
                fail: { [weak self] error in
                    self?.uiStore.showError(error)
                }
            )
        )
        observations[key] = Observation(
            identity: identity,
            subscription: subscription
        )
    }

    private func deliver(
        _ detail: BridgeImportCandidateDetail?,
        key: String,
        identity: UUID
    ) {
        guard observations[key]?.identity == identity else { return }
        guard let detail else {
            // The key names no scanned folder any more, so nothing can be done
            // with it: drop it from the selection, which closes this read.
            uiStore.removeFolderCandidateSelection([key])
            return
        }
        let isFirstRead = importStore.selectedCandidates[key] == nil
        importStore.applyCandidateDetail(key: key, detail: detail)
        guard isFirstRead,
            let candidate = importStore.selectedCandidates[key],
            case .idle = shownIdentifyState(
                resumed: candidate.resumedIdentifyState,
                runtime: importer.candidateRuntime(key)
            )
        else { return }
        // The first selection of a folder nothing has run for starts its
        // identification; a later selection finds a state that is no longer
        // idle and starts nothing.
        importer.autoIdentifyFolder(key)
    }

    deinit {
        for observation in observations.values {
            observation.subscription.cancel()
        }
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
        uiStore: UiStore,
        importer: Importer
    ) {
        self.appHandle = appHandle
        self.importStore = importStore
        self.outputStore = outputStore
        self.uiStore = uiStore
        selection = ImportSelectionObservations(
            appHandle: appHandle,
            importStore: importStore,
            uiStore: uiStore,
            importer: importer
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
