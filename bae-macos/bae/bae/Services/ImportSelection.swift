import BaeKit
import Foundation
import Observation
import os.log

private let importSelectionLogger = Logger.bae("ImportSelection")

/// How far a bulk action over the selection has got.
struct ImportCandidateActionProgress {
    let action: BridgeCandidateAction
    let total: Int
    var completed: Int
}

/// The calls the selection makes, as values a test can replace.
struct ImportSelectionOperations: Sendable {
    /// Returns the selection revision of the list value that reflects the
    /// change.
    let change:
        @Sendable (BridgeImportListView, BridgeSelectionChange) async throws ->
            UInt64
    let selectAll: @Sendable (BridgeImportListView) async throws -> Void
    let keepShown: @Sendable (BridgeImportListView) async throws -> Void
    let sourceFolders: @Sendable () async throws -> [String]
    let combine: @Sendable () async throws -> String
    let run:
        @Sendable (
            BridgeImportListView,
            BridgeCandidateAction,
            @escaping @Sendable (BridgeSelectionActionProgress) -> Void
        ) async throws -> [BridgeSelectionActionFailure]

    static func live(handle: any AppHandleProtocol) -> ImportSelectionOperations
    {
        ImportSelectionOperations(
            change: {
                try await handle.changeImportSelection(view: $0, change: $1)
            },
            selectAll: {
                try await handle.selectAllImportCandidates(view: $0)
            },
            keepShown: {
                try await handle.keepShownImportSelection(view: $0)
            },
            sourceFolders: {
                try await handle.importSelectionSourceFolders()
            },
            combine: { try await handle.combineImportSelection() },
            run: { view, action, progress in
                try await handle.runImportSelectionAction(
                    view: view,
                    action: action,
                    progress: SelectionActionProgressSink(apply: progress)
                )
            }
        )
    }

    /// Operations that change nothing, for previews and tests that never act
    /// on the selection.
    static let inert = ImportSelectionOperations(
        change: { _, _ in 0 },
        selectAll: { _ in },
        keepShown: { _ in },
        sourceFolders: { [] },
        combine: { throw StubError.notImplemented },
        run: { _, _, _ in [] }
    )
}

private final class SelectionActionProgressSink:
    SelectionActionProgressCallback,
    @unchecked Sendable
{
    private let apply: @Sendable (BridgeSelectionActionProgress) -> Void

    init(apply: @escaping @Sendable (BridgeSelectionActionProgress) -> Void) {
        self.apply = apply
    }

    func onProgress(progress: BridgeSelectionActionProgress) {
        apply(progress)
    }
}

/// The import list's selection. Core holds it as rows, so a selection of any
/// size is one core call to change and one to act on; what it holds and can
/// be told to do arrives here as core's summary, and this keeps no selection
/// of its own.
@MainActor
@Observable
final class ImportSelection {
    /// What the selection holds and can be told to do, as core last said.
    private(set) var summary = BridgeSelectionSummary(
        count: 0,
        single: nil,
        offers: []
    )
    /// How far the running bulk action has got, while one runs.
    private(set) var progress: ImportCandidateActionProgress?
    @ObservationIgnored
    private var task: Task<Void, Never>?
    /// The run core's progress reports belong to; a report that arrives once
    /// its run has ended is dropped.
    @ObservationIgnored
    private var currentRun: UUID?
    @ObservationIgnored
    private let operations: ImportSelectionOperations

    init(operations: ImportSelectionOperations = .inert) {
        self.operations = operations
    }

    var isRunning: Bool { task != nil || progress != nil }

    /// Take core's latest summary.
    func apply(_ summary: BridgeSelectionSummary) {
        guard summary != self.summary else { return }
        self.summary = summary
    }

    /// Apply `change`, and return the selection revision of the list value
    /// that reflects it.
    @discardableResult
    func change(
        in view: BridgeImportListView,
        _ change: BridgeSelectionChange
    ) async throws -> UInt64 {
        try await operations.change(view, change)
    }

    /// Select every row `view` shows, whether or not its page has loaded.
    func selectAll(in view: BridgeImportListView) async throws {
        try await operations.selectAll(view)
    }

    /// Drop the selected rows `view` does not show.
    func keepShown(in view: BridgeImportListView) async throws {
        try await operations.keepShown(view)
    }

    func sourceFolders() async throws -> [String] {
        try await operations.sourceFolders()
    }

    /// Read every selected folder as one release, which core selects.
    func combine() async throws -> String {
        try await operations.combine()
    }

    /// Run `action` over every selected row that offers it, in the order
    /// `view` shows them, after `before`; the rows it fails on are reported
    /// together once it ends.
    @discardableResult
    func start(
        _ action: BridgeCandidateAction,
        in view: BridgeImportListView,
        uiStore: UiStore,
        before: @escaping @MainActor () async -> Void
    ) -> Task<Void, Never>? {
        guard !isRunning else { return nil }
        task = Task {
            defer { task = nil }
            await before()
            guard !Task.isCancelled else { return }
            await perform(action, in: view, uiStore: uiStore)
        }
        return task
    }

    func cancel() { task?.cancel() }

    func perform(
        _ action: BridgeCandidateAction,
        in view: BridgeImportListView,
        uiStore: UiStore
    ) async {
        let run = UUID()
        currentRun = run
        progress = ImportCandidateActionProgress(
            action: action,
            total: 0,
            completed: 0
        )
        defer {
            currentRun = nil
            progress = nil
        }
        let failures: [BridgeSelectionActionFailure]
        do {
            failures = try await operations.run(view, action) { reported in
                Task { @MainActor [weak self] in
                    guard let self, self.currentRun == run,
                        Int(reported.completed)
                            >= self.progress?.completed ?? 0
                    else { return }
                    self.progress = ImportCandidateActionProgress(
                        action: action,
                        total: Int(reported.total),
                        completed: Int(reported.completed)
                    )
                }
            }
        }
        catch is CancellationError { return }
        catch {
            uiStore.showError(error)
            return
        }
        let reported = failures.compactMap { failure -> DisplayError? in
            importSelectionLogger.error(
                "\(String(describing: action)) failed for \(failure.candidateKey): \(String(reflecting: failure.error))"
            )
            return DisplayError(failure.error)?.addingContext(failure.name)
        }
        guard !reported.isEmpty else { return }
        let details = reported.compactMap(\.detail)
        uiStore.showError(
            DisplayError(
                line: reported.map(\.line).joined(separator: "\n"),
                detail: details.isEmpty
                    ? nil : details.joined(separator: "\n\n")
            )
        )
    }
}
