import BaeKit

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

private final class ImportCandidatesValueSink: ImportCandidatesCallback,
    @unchecked Sendable
{
    private let apply:
        @MainActor @Sendable (BridgeImportCandidatesSnapshot) -> Void
    private let applyRuntime:
        @MainActor @Sendable (BridgeCandidateRuntimeChange) -> Void
    private let fail: @MainActor @Sendable (BridgeError) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeImportCandidatesSnapshot)
            -> Void,
        applyRuntime:
            @escaping @MainActor @Sendable (BridgeCandidateRuntimeChange)
            -> Void,
        fail: @escaping @MainActor @Sendable (BridgeError) -> Void
    ) {
        self.apply = apply
        self.applyRuntime = applyRuntime
        self.fail = fail
    }

    func onValue(value: BridgeImportCandidatesSnapshot) {
        Task { @MainActor in apply(value) }
    }

    func onRuntime(change: BridgeCandidateRuntimeChange) {
        Task { @MainActor in applyRuntime(change) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

private final class ImportTriageValueSink: ImportTriageCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeTriageQueue) -> Void
    private let fail: @MainActor @Sendable (BridgeError) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeTriageQueue) -> Void,
        fail: @escaping @MainActor @Sendable (BridgeError) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeTriageQueue) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

@MainActor
final class DesktopSubscriptions {
    private let appHandle: AppHandle
    private let importStore: ImportStore
    private let outputStore: OutputStore
    private let uiStore: UiStore
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
    }

    func start() {
        precondition(subscriptions.isEmpty)
        subscriptions = [
            appHandle.subscribeOutputs(
                callback: OutputValueSink { [outputStore] value in
                    outputStore.applySnapshot(value)
                }
            ),
            appHandle.subscribeImportCandidates(
                callback: ImportCandidatesValueSink(
                    apply: { [importStore, uiStore] value in
                        importStore.applyImportCandidatesSnapshot(value)
                        uiStore.retainFolderCandidateSelection(
                            in: Set(importStore.folderCandidates.keys)
                        )
                    },
                    applyRuntime: { [importStore] change in
                        importStore.applyCandidateRuntimeChange(change)
                    },
                    fail: { [uiStore] error in
                        uiStore.showError(error)
                    }
                )
            ),
            appHandle.subscribeImportTriage(
                callback: ImportTriageValueSink(
                    apply: { [importStore] value in
                        importStore.triageQueue = value
                    },
                    fail: { [uiStore] error in
                        uiStore.showError(error)
                    }
                )
            ),
        ]
    }
}
