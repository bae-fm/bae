import BaeKit
import Combine
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

private final class ImportSelectionSink: ImportSelectionCallback,
    @unchecked Sendable
{
    let apply: @MainActor @Sendable (BridgeImportSelection) -> Void
    let fail: @MainActor @Sendable (BridgeError) -> Void
    init(
        apply: @escaping @MainActor @Sendable (BridgeImportSelection) -> Void,
        fail: @escaping @MainActor @Sendable (BridgeError) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }
    func onValue(value: BridgeImportSelection) {
        Task { @MainActor in apply(value) }
    }
    func onError(error: BridgeError) { Task { @MainActor in fail(error) } }
}

/// One selected-key query and the editor read for the single visible pane.
@MainActor
final class ImportSelectionObservations {
    private struct Observation {
        let identity: UUID
        let subscription: LiveSubscription
    }
    private let appHandle: AppHandle
    private let importStore: ImportStore
    private let uiStore: UiStore
    private var keys: Set<String> = []
    private var selection: Observation?
    private var editor: Observation?
    private var editorKey: String?
    private var editorVisible = false

    init(appHandle: AppHandle, importStore: ImportStore, uiStore: UiStore) {
        self.appHandle = appHandle
        self.importStore = importStore
        self.uiStore = uiStore
    }

    func selectionChanged(_ keys: Set<String>) {
        guard self.keys != keys else { return }
        self.keys = keys
        selection?.subscription.cancel()
        selection = nil
        importStore.selection = nil
        updateEditor()
        guard !keys.isEmpty else { return }
        let identity = UUID()
        let subscription = appHandle.subscribeImportSelection(
            candidateKeys: Array(keys),
            callback: ImportSelectionSink(
                apply: { [weak self] value in
                    guard let self, self.selection?.identity == identity else {
                        return
                    }
                    self.importStore.selection = value
                    self.uiStore.removeFolderCandidateSelection(
                        keys.subtracting(value.candidateKeys)
                    )
                },
                fail: { [weak self] error in
                    guard let self, self.selection?.identity == identity else {
                        return
                    }
                    self.uiStore.showError(error)
                }
            )
        )
        selection = Observation(identity: identity, subscription: subscription)
    }

    func setEditorVisible(_ visible: Bool) {
        editorVisible = visible
        updateEditor()
    }

    private func updateEditor() {
        let key = editorVisible && keys.count == 1 ? keys.first : nil
        guard editorKey != key else { return }
        editor?.subscription.cancel()
        editor = nil
        editorKey = key
        importStore.clearEditor()
        guard let key else { return }
        let identity = UUID()
        let subscription = appHandle.subscribeImportCandidate(
            candidateKey: key,
            callback: ImportCandidateSink(
                apply: { [weak self] detail in
                    guard let self, self.editor?.identity == identity else {
                        return
                    }
                    guard let detail else {
                        self.uiStore.removeFolderCandidateSelection([key])
                        return
                    }
                    self.importStore.applyCandidateDetail(
                        key: key,
                        detail: detail
                    )
                },
                fail: { [weak self] error in
                    guard let self, self.editor?.identity == identity else {
                        return
                    }
                    self.uiStore.showError(error)
                }
            )
        )
        editor = Observation(identity: identity, subscription: subscription)
    }

    deinit {
        selection?.subscription.cancel()
        editor?.subscription.cancel()
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
    private var editorVisibility: AnyCancellable?

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
        uiStore.onFolderCandidateSelectionChanged = { [weak selection] keys in
            selection?.selectionChanged(keys)
        }
        editorVisibility = importStore.editorVisibility.sink {
            [selection] visible in
            selection.setEditorVisible(visible)
        }
        selection.selectionChanged(uiStore.selectedFolderCandidates)
        importList.startLoad()
    }
}
