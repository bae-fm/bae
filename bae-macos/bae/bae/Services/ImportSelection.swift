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

/// The calls the selection makes, as values a test can replace. Each goes to
/// the import list that owns the selection.
struct ImportSelectionOperations: Sendable {
    /// Returns the selection revision of the list value that reflects the
    /// change.
    let change: @Sendable (BridgeSelectionChange) async throws -> UInt64
    let selectAll: @Sendable () async throws -> Void
    let sourceFolders: @Sendable () async throws -> [String]
    let combine: @Sendable () async throws -> String
    let separate: @Sendable () async throws -> [String]
    let run:
        @Sendable (
            BridgeCandidateAction,
            @escaping @Sendable (BridgeSelectionActionProgress) -> Void
        ) async throws -> [BridgeSelectionActionFailure]

    static func live(list: ImportListSelectionOwner)
        -> ImportSelectionOperations
    {
        ImportSelectionOperations(
            change: { try await list.current().changeSelection(change: $0) },
            selectAll: { _ = try await list.current().selectAll() },
            sourceFolders: {
                try await list.current().selectionSourceFolders()
            },
            combine: { try await list.current().combineSelection() },
            separate: { try await list.current().separateSelection() },
            run: { action, progress in
                try await list.current()
                    .runSelectionAction(
                        action: action,
                        progress: SelectionActionProgressSink(apply: progress)
                    )
            }
        )
    }

    /// Operations that change nothing, for previews and tests that never act
    /// on the selection.
    static let inert = ImportSelectionOperations(
        change: { _ in 0 },
        selectAll: {},
        sourceFolders: { [] },
        combine: { throw StubError.notImplemented },
        separate: { throw StubError.notImplemented },
        run: { _, _ in [] }
    )
}

/// The import list the selection belongs to: core's list subscription owns
/// the selection beside the rows it shows, so what a person does to the
/// selection goes to the list the sidebar reads now. A list rebuilt after a
/// failed read is a new list, with a selection of its own.
final class ImportListSelectionOwner: @unchecked Sendable {
    private let lock = NSLock()
    private var list: (any ImportListSubscriptionProtocol)?

    /// Make `list` the one the selection's calls go to.
    func own(_ list: any ImportListSubscriptionProtocol) {
        lock.withLock { self.list = list }
    }

    func current() throws -> any ImportListSubscriptionProtocol {
        guard let list = lock.withLock({ self.list }) else {
            throw ImportListSelectionOwnerError.noList
        }
        return list
    }
}

/// The selection was acted on before the import list was read.
enum ImportListSelectionOwnerError: Error {
    case noList
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

/// The import list's selection. Core's import list holds it, and only ever
/// with rows the list shows, so a selection of any size is one core call to
/// change and one to act on; what it holds and can be told to do arrives here
/// with each of the list's values, and this keeps no selection of its own.
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
    func change(_ change: BridgeSelectionChange) async throws -> UInt64 {
        try await operations.change(change)
    }

    /// Select every row the list shows, whether or not its page has loaded.
    func selectAll() async throws {
        try await operations.selectAll()
    }

    func sourceFolders() async throws -> [String] {
        try await operations.sourceFolders()
    }

    /// Read every selected folder as one release, which core selects.
    func combine() async throws -> String {
        try await operations.combine()
    }

    /// Read the one selected release as the releases it is made of, which
    /// core selects.
    @discardableResult
    func separate() async throws -> [String] {
        try await operations.separate()
    }

    /// Run `action` over every selected row that offers it, in the order
    /// the list shows them, after `before`; the rows it fails on are reported
    /// together once it ends.
    @discardableResult
    func start(
        _ action: BridgeCandidateAction,
        uiStore: UiStore,
        before: @escaping @MainActor () async -> Void
    ) -> Task<Void, Never>? {
        guard !isRunning else { return nil }
        task = Task {
            defer { task = nil }
            await before()
            guard !Task.isCancelled else { return }
            await perform(action, uiStore: uiStore)
        }
        return task
    }

    func cancel() { task?.cancel() }

    func perform(
        _ action: BridgeCandidateAction,
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
            failures = try await operations.run(action) { reported in
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
