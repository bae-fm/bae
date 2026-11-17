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
/// the import list the sidebar reads now, with the keys the screen keeps
/// selected.
struct ImportSelectionOperations: Sendable {
    /// The keys of the rows the list shows on a tab in an order, the ones
    /// the screen shows, from one row to another, both included: what a
    /// Shift run selects.
    let keysBetween:
        @Sendable (BridgeTriageTab, BridgeImportListOrder, Data, Data)
            async throws -> [Data]
    /// The keys of every row the list shows: what Select All selects.
    let allKeys: @Sendable () async throws -> [Data]
    let sourceFolders: @Sendable ([Data]) async throws -> [Data]
    let combine: @Sendable ([Data]) async throws -> Data
    let separate: @Sendable ([Data]) async throws -> [Data]
    /// A new stream for one action's progress: unfed until `run` feeds it
    /// from the list it runs on.
    let progressStream:
        @Sendable () -> LatestStream<
            BridgeSelectionActionProgress
        >
    let run:
        @Sendable (
            BridgeCandidateAction,
            [Data],
            LatestStream<BridgeSelectionActionProgress>
        ) async throws -> [BridgeSelectionActionFailure]

    static func live(list: ImportListSelectionOwner)
        -> ImportSelectionOperations
    {
        ImportSelectionOperations(
            keysBetween: { tab, order, from, to in
                try await list.current()
                    .keysBetween(
                        tab: tab,
                        order: order,
                        from: from,
                        to: to
                    )
            },
            allKeys: { try await list.current().keys() },
            sourceFolders: {
                try await list.current().selectionSourceFolders(selected: $0)
            },
            combine: {
                try await list.current().combineSelection(selected: $0)
            },
            separate: {
                try await list.current().separateSelection(selected: $0)
            },
            progressStream: { .bulkActionProgress() },
            run: { action, selected, progress in
                let list = try list.current()
                let feed = progress.feed(from: list)
                defer { feed.cancel() }
                return try await list.runSelectionAction(
                    action: action,
                    selected: selected
                )
            }
        )
    }

    /// Operations that change nothing, for shots and tests that never act
    /// on the selection.
    static let inert = ImportSelectionOperations(
        keysBetween: { _, _, _, _ in [] },
        allKeys: { [] },
        sourceFolders: { _ in [] },
        combine: { _ in throw StubError.notImplemented },
        separate: { _ in throw StubError.notImplemented },
        progressStream: { .bulkActionProgress() },
        run: { _, _, _ in [] }
    )
}

/// The import list the selection's calls go to: the list the sidebar reads
/// now. A list rebuilt after a failed read is a new list, and the selection
/// starts empty on it.
final class ImportListSelectionOwner: @unchecked Sendable {
    private let lock = NSLock()
    private var list: (any ImportListProtocol)?

    /// Make `list` the one the selection's calls go to.
    func own(_ list: any ImportListProtocol) {
        lock.withLock { self.list = list }
    }

    func current() throws -> any ImportListProtocol {
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

/// The import list's selection, which the screen keeps: the keys of the rows
/// the person selected. Each row the table draws takes its highlight from
/// them. What they hold and can be told to do arrives as core's live read,
/// asked for these keys whatever the list shows, and each action on them
/// sends them, for core to act on the ones the list shows.
@MainActor
@Observable
final class ImportSelection {
    /// The keys of the rows the person selected. A row the list hides by a
    /// change to the row itself keeps its key here.
    private(set) var keys: Set<Data> = []
    /// What the selected rows hold and can be told to do, as core last said.
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
    /// selection holds now. Core reads the rows a Shift-click adds on `tab`
    /// in `order`, the tab and order the screen shows.
    func edit(
        _ edit: ImportListSelectionRequest.Edit,
        on tab: BridgeTriageTab,
        in order: BridgeImportListOrder
    ) {
        switch edit {
        case .only(let keys):
            select(keys)
        case .adjust(let add, let remove, let range):
            select(keys.union(add).subtracting(remove))
            guard let range else { return }
            readKeys(adding: true) {
                try await $0.keysBetween(tab, order, range.from, range.to)
            }
        }
    }

    /// Select every row the list shows, whether or not its page has loaded,
    /// once core reads their keys.
    func selectAll() {
        readKeys(adding: false) { try await $0.allKeys() }
    }

    /// Read keys from core with `read`, and add them to the selection or
    /// make them the selection. A change to the selection before core
    /// answers cancels the read, and the answer then changes nothing.
    private func readKeys(
        adding: Bool,
        _ read:
            @escaping @Sendable (ImportSelectionOperations) async throws ->
            [Data]
    ) {
        let operations = operations
        keysRead.replace(
            "Selection keys read",
            work: { try await read(operations) },
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
        try await operations.sourceFolders(keys.inByteOrder)
    }

    /// Read every selected folder the list shows as one release, and select
    /// it.
    func combine() async throws -> Data {
        let key = try await operations.combine(keys.inByteOrder)
        select([key])
        return key
    }

    /// Read the one selected release as the releases it is made of, and
    /// select them.
    @discardableResult
    func separate() async throws -> [Data] {
        let separated = try await operations.separate(keys.inByteOrder)
        select(Set(separated))
        return separated
    }

    /// Run `action` over every selected row the list shows that offers it,
    /// in the order the list shows them, after `before`; the rows it fails
    /// on are reported together once it ends.
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

extension Set<Data> {
    /// The keys in the order of their bytes, so the same keys always cross in
    /// one order.
    var inByteOrder: [Data] {
        sorted { $0.lexicographicallyPrecedes($1) }
    }
}
