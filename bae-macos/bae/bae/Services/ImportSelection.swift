import BaeKit
import Foundation
import Observation
import os.log

private let importSelectionLogger = Logger.bae("ImportSelection")

/// A bulk action over the selection while it runs: what it does, and the
/// stream of how far it has got, which only the native line drawing it
/// reads.
struct RunningSelectionAction {
    let action: BridgeCandidateAction
    let progress: LatestSource<BridgeSelectionActionProgress>
}

/// The calls the selection makes, as values a test can replace. Each goes to
/// the import list with the request the sidebar shows and the keys the screen
/// keeps selected.
struct ImportSelectionOperations: Sendable {
    /// The keys of the rows the list `request` shows from one row to another,
    /// both included: what Shift-clicking a row selects.
    let keysBetween:
        @Sendable (BridgeImportListRequest, Data, Data) async throws -> [Data]
    /// The keys of every row the list `request` shows: what Select All
    /// selects.
    let allKeys: @Sendable (BridgeImportListRequest) async throws -> [Data]
    let sourceFolders:
        @Sendable (BridgeImportListRequest, [Data]) async throws -> [Data]
    let combine:
        @Sendable (BridgeImportListRequest, [Data]) async throws -> Data
    let separate:
        @Sendable (BridgeImportListRequest, [Data]) async throws -> [Data]
    /// A new stream for one action's progress: unfed until `run` feeds it
    /// from the list it runs on.
    let progressStream:
        @Sendable () -> LatestStream<
            BridgeSelectionActionProgress
        >
    let run:
        @Sendable (
            BridgeImportListRequest,
            BridgeCandidateAction,
            [Data],
            LatestStream<BridgeSelectionActionProgress>
        ) async throws -> [BridgeSelectionActionFailure]

    static func live(list: any ImportListProtocol)
        -> ImportSelectionOperations
    {
        ImportSelectionOperations(
            keysBetween: { request, from, to in
                try await list.keysBetween(request: request, from: from, to: to)
            },
            allKeys: { try await list.keys(request: $0) },
            sourceFolders: {
                try await list.selectionSourceFolders(
                    request: $0,
                    selected: $1
                )
            },
            combine: {
                try await list.combineSelection(request: $0, selected: $1)
            },
            separate: {
                try await list.separateSelection(request: $0, selected: $1)
            },
            progressStream: { .bulkActionProgress() },
            run: { request, action, selected, progress in
                let feed = progress.feed(from: list)
                defer { feed.cancel() }
                return try await list.runSelectionAction(
                    request: request,
                    action: action,
                    selected: selected
                )
            }
        )
    }

    /// Operations that change nothing, for shots and tests that never act
    /// on the selection.
    static let inert = ImportSelectionOperations(
        keysBetween: { _, _, _ in [] },
        allKeys: { _ in [] },
        sourceFolders: { _, _ in [] },
        combine: { _, _ in throw StubError.notImplemented },
        separate: { _, _ in throw StubError.notImplemented },
        progressStream: { .bulkActionProgress() },
        run: { _, _, _, _ in [] }
    )
}

/// The import list's selection, which the screen keeps: the keys of the rows
/// the person selected. Each row the table draws takes its highlight from
/// them. ImportSelection asks core's live read about these keys, and the read
/// says what the selected rows hold and which actions they offer, whether or
/// not the list shows them. Each action sends the keys, and core acts on the
/// ones the list shows.
@MainActor
@Observable
final class ImportSelection {
    /// The keys of the rows the person selected. A row the list hides by a
    /// change to the row itself keeps its key here.
    private(set) var keys: Set<Data> = []
    /// What the selected rows hold and which actions they offer, as core
    /// last said.
    private(set) var summary = BridgeSelectionSummary(
        count: 0,
        single: nil,
        offers: []
    )
    /// The running bulk action, while one runs.
    private(set) var running: RunningSelectionAction?
    @ObservationIgnored
    private var task: Task<Void, Never>?
    @ObservationIgnored
    private let operations: ImportSelectionOperations
    /// The read of what the selected rows hold, which takes the keys.
    @ObservationIgnored
    private var query: ImportSelectionQuery?
    /// The read of the keys a Shift-click or Select All adds, while core
    /// reads them. The person's next change to the selection cancels it.
    @ObservationIgnored
    private let keysRead = CancellableTaskSlot()
    /// Counts the lists the selection has been on, so an answer read for an
    /// earlier list changes nothing.
    @ObservationIgnored
    private(set) var generation = 0
    /// The request of the list the sidebar shows, which each read of rows
    /// takes; nil until the sidebar shows its first list.
    @ObservationIgnored
    private(set) var request: BridgeImportListRequest?
    /// Where a failure to tell the read the keys goes.
    @ObservationIgnored
    var onError: @MainActor (any Error) -> Void = { _ in }

    init(operations: ImportSelectionOperations = .inert) {
        self.operations = operations
    }

    var isRunning: Bool { task != nil || running != nil }

    /// Read what the selected rows hold through `query`, on a list the
    /// selection starts empty on.
    func attach(_ query: ImportSelectionQuery?) {
        keysRead.cancel()
        generation += 1
        self.query = query
        keys = []
    }

    /// Take `request` as the list the sidebar shows: the reads and actions
    /// that follow take it.
    func show(_ request: BridgeImportListRequest) {
        self.request = request
    }

    /// The request of the list the sidebar shows. Before the sidebar shows a
    /// list there are no rows to act on, so a read or an action fails.
    private func shownRequest() throws -> BridgeImportListRequest {
        guard let request else { throw ImportSelectionError.noList }
        return request
    }

    /// Take core's latest summary.
    func apply(_ summary: BridgeSelectionSummary) {
        guard summary != self.summary else { return }
        self.summary = summary
    }

    /// Make `keys` the selection. A read for a Shift-click or Select All
    /// that core has not answered yet stops, because the newer choice
    /// replaces it.
    func select(_ keys: Set<Data>) {
        keysRead.cancel()
        replace(keys)
    }

    /// Make `edit`, the change a click or key made, to the keys the
    /// selection holds now. Core reads the rows a Shift-click adds in the
    /// list the screen shows.
    func edit(_ edit: ImportListSelectionRequest.Edit) {
        switch edit {
        case .only(let keys):
            select(keys)
        case .adjust(let add, let remove, let range):
            select(keys.union(add).subtracting(remove))
            guard let range else { return }
            readKeys(adding: true) { operations, request in
                try await operations.keysBetween(request, range.from, range.to)
            }
        }
    }

    /// Select every row the list shows, whether or not its page has loaded,
    /// once core reads their keys.
    func selectAll() {
        readKeys(adding: false) { operations, request in
            try await operations.allKeys(request)
        }
    }

    /// Read keys from core with `read`, and add them to the selection or
    /// make them the selection. A change to the selection before core
    /// answers cancels the read, and the answer then changes nothing.
    private func readKeys(
        adding: Bool,
        _ read:
            @escaping @Sendable (
                ImportSelectionOperations, BridgeImportListRequest
            ) async throws -> [Data]
    ) {
        let operations = operations
        let request: BridgeImportListRequest
        do {
            request = try shownRequest()
        }
        catch {
            onError(error)
            return
        }
        keysRead.replace(
            "Selection keys read",
            work: { try await read(operations, request) },
            onSuccess: { [weak self] found in
                guard let self else { return }
                replace(adding ? keys.union(found) : Set(found))
            },
            onError: { [weak self] in self?.onError($0) }
        )
    }

    private func replace(_ keys: Set<Data>) {
        guard keys != self.keys else { return }
        self.keys = keys
        do {
            try query?.setSelected(keys.inByteOrder)
        }
        catch {
            onError(error)
        }
    }

    /// Take `left` out of the selection.
    func remove(_ left: [Data]) {
        guard !left.isEmpty else { return }
        select(keys.subtracting(left))
    }

    func sourceFolders() async throws -> [Data] {
        try await operations.sourceFolders(try shownRequest(), keys.inByteOrder)
    }

    /// Read every selected folder the list shows as one release, and select
    /// it.
    func combine() async throws -> Data {
        let key = try await operations.combine(
            try shownRequest(),
            keys.inByteOrder
        )
        select([key])
        return key
    }

    /// Read the one selected release as the releases it is made of, and
    /// select them.
    @discardableResult
    func separate() async throws -> [Data] {
        let separated = try await operations.separate(
            try shownRequest(),
            keys.inByteOrder
        )
        select(Set(separated))
        return separated
    }

    /// Run `action` over every selected row the list shows that offers it,
    /// in the order the list shows them, after `before`. Once it ends, the
    /// app shows one error that lists every row it failed on.
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
        let progress = operations.progressStream()
        running = RunningSelectionAction(
            action: action,
            progress: progress.only
        )
        defer { running = nil }
        let failures: [BridgeSelectionActionFailure]
        do {
            failures = try await operations.run(
                try shownRequest(),
                action,
                keys.inByteOrder,
                progress
            )
        }
        catch is CancellationError { return }
        catch {
            uiStore.showError(error)
            return
        }
        let reported = failures.compactMap { failure -> DisplayError? in
            importSelectionLogger.error(
                "\(String(describing: action)) failed for \(bridgeCandidateKeyText(candidateKey: failure.candidateKey)): \(String(reflecting: failure.error))"
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

/// The selection was read or acted on before the sidebar showed a list.
enum ImportSelectionError: Error {
    case noList
}

extension Set<Data> {
    /// The keys in the order of their bytes, so core always gets the same
    /// keys in one order.
    var inByteOrder: [Data] {
        sorted { $0.lexicographicallyPrecedes($1) }
    }
}
