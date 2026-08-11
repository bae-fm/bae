import BaeKit

@MainActor
final class DesktopProjections {
    private struct ImportCandidateValue: Sendable {
        let key: String
        let snapshot: BridgeImportCandidateSnapshot?
    }

    private let registry: ProjectionRegistry
    private let outputs: Outputs
    private let outputStore: OutputStore
    private let importer: Importer
    private let importStore: ImportStore
    private let uiStore: UiStore
    private var registrations: [ProjectionRegistration] = []

    init(
        registry: ProjectionRegistry,
        outputs: Outputs,
        outputStore: OutputStore,
        importer: Importer,
        importStore: ImportStore,
        uiStore: UiStore
    ) {
        self.registry = registry
        self.outputs = outputs
        self.outputStore = outputStore
        self.importer = importer
        self.importStore = importStore
        self.uiStore = uiStore
    }

    func start() {
        precondition(registrations.isEmpty)
        registrations = [
            registry.register(makeExportProjection()),
            registry.register(makeImportCandidatesProjection()),
            registry.register(makeImportCandidateProjection()),
            registry.register(makeImportTriageQueueProjection()),
            registry.register(makeImportLibraryStatusProjection()),
        ]
    }

    private func makeExportProjection() -> Projection<BridgeOutputSnapshot> {
        Projection(
            domain: .outputQueue,
            query: { [outputs] _ in try await outputs.outputSnapshot() },
            apply: { [outputStore] snapshot in
                outputStore.applySnapshot(snapshot)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeImportCandidatesProjection()
        -> Projection<BridgeImportCandidatesSnapshot>
    {
        Projection(
            domains: [.importCandidateList, .watchedFolders],
            query: { [importer] _ in try await importer.importCandidates() },
            apply: { [importStore, uiStore] snapshot in
                importStore.applyImportCandidatesSnapshot(snapshot)
                uiStore.retainFolderCandidateSelection(
                    in: Set(importStore.folderCandidates.keys)
                )
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeImportTriageQueueProjection() -> Projection<
        BridgeTriageQueue
    > {
        Projection(
            domains: [
                .importCandidateList, .importCandidate, .watchedFolders,
                .release,
            ],
            query: { [importer] _ in try await importer.importTriageQueue() },
            apply: { [importStore] queue in
                importStore.triageQueue = queue
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeImportCandidateProjection()
        -> Projection<ImportCandidateValue>
    {
        Projection(
            domain: .importCandidate,
            query: { [importer] invalidation in
                guard case .importCandidate(let key) = invalidation else {
                    preconditionFailure(
                        "Import candidate projection received \(invalidation)"
                    )
                }
                let snapshot = try await importer.candidate(key)
                return ImportCandidateValue(key: key, snapshot: snapshot)
            },
            apply: { [importStore, uiStore] value in
                importStore.applyImportCandidateSnapshot(
                    key: value.key,
                    snapshot: value.snapshot
                )
                uiStore.retainFolderCandidateSelection(
                    in: Set(importStore.folderCandidates.keys)
                )
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeImportLibraryStatusProjection() -> Projection<String> {
        Projection(
            domain: .release,
            query: { invalidation in
                guard case .release(let releaseId) = invalidation else {
                    preconditionFailure(
                        "Import library-status projection received \(invalidation)"
                    )
                }
                return releaseId
            },
            apply: { [importStore] releaseId in
                importStore.removeLibraryStatus(releaseId: releaseId)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }
}
