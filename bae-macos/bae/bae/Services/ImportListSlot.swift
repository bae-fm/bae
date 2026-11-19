import BaeKit
import Combine
import Foundation
import Observation

/// What narrows the import list's rows: the tab, the typed text and Found's
/// filter entry. A count belongs to the narrowing its list was read under.
struct ImportListNarrowing: Equatable {
    let tab: BridgeTriageTab
    let filterText: String
    let pendingFilter: BridgePendingFilter

    init(of view: BridgeImportListView) {
        tab = view.tab
        filterText = view.filterText
        pendingFilter = view.pendingFilter
    }

    init(
        tab: BridgeTriageTab,
        filterText: String,
        pendingFilter: BridgePendingFilter
    ) {
        self.tab = tab
        self.filterText = filterText
        self.pendingFilter = pendingFilter
    }
}

/// Holds the import sidebar's `PaginatedList`, the reads around it and the
/// view and folds it shows. A change to the view or the folds opens a new
/// list under the new request.
///
/// The view (which tab, which filter text, which order) and the groups
/// folded shut decide which items sit at which positions, so the slot keeps
/// them and every read of the list takes them in its request. When the person
/// changes either, the slot reads the new request's first rows and count in a
/// new list, and the sidebar keeps drawing the old list until the new one has
/// both. Core decides what each change comes to from the request before and
/// the request after: a new tab, filter or search opens at the first selected
/// row it shows, or its top, and takes out the selected rows it doesn't show;
/// a new order or fold opens at the row on top, and a fold takes out the
/// selected rows it hid.
///
/// `UiStore` keeps the tab and the filter text as session state, so every
/// setter of them here writes both: the store the sidebar renders from, and
/// the view the list reads.
/// The list on screen: the handle its reads go through, the list, and the
/// request it reads. The slot sets the three at once, so none is there without
/// the others.
struct ShownImportList {
    let handle: ImportListHandle
    let list: PaginatedList<BridgeImportListItem>
    let request: BridgeImportListRequest
}

/// The slot was asked to reveal a candidate with no list to show it in: its
/// first read had not started, or ended without a list.
enum ImportListSlotError: Error {
    case noList
}

@MainActor
@Observable
final class ImportListSlot {
    /// The list on screen, once the slot has read one.
    private(set) var current: ShownImportList?
    private(set) var sortOrder: BridgeImportListOrder
    /// The row each opening of the list puts on top: its first selected row,
    /// or its top. The table scrolls to each position `openings` sends. A
    /// table not shown then finds the opening in `place`.
    @ObservationIgnored
    let openings = PassthroughSubject<Int, Never>()
    /// Where the person is in the list, as the table last recorded it. Held
    /// here rather than by the table so the import tab finds it again after
    /// another tab was shown. Nothing draws from it.
    @ObservationIgnored
    var place: ImportListPlace?
    @ObservationIgnored
    private let defaults: UserDefaults
    private static let sortPreferenceKey = "importCandidateSortOrder"

    /// The view the person asked for.
    @ObservationIgnored
    private(set) var view: BridgeImportListView
    /// The groups the person folded shut.
    @ObservationIgnored
    private var folds: BridgeImportListFolds = .allOpen
    @ObservationIgnored
    private let makeHandle: () async throws -> ImportListHandle
    @ObservationIgnored
    private let importStore: ImportStore
    @ObservationIgnored
    private let uiStore: UiStore
    /// The keys of the rows the person selected, which the table draws its
    /// highlight from and each change of the view and the folds passes to
    /// core.
    let selection: ImportSelection
    /// Where the selection's values go beside the selection the panes read.
    @ObservationIgnored
    private let onSelection:
        @MainActor @Sendable (BridgeSelectionSummary) -> Void
    /// The reads around the list: the summary, Found's filter entries and
    /// the selection.
    @ObservationIgnored
    private var surroundings: [any Sendable] = []
    @ObservationIgnored
    private var reloadTask: Task<Void, Never>?
    /// How many reveals wait for the slot's first list. A test waits on it,
    /// under observation, to know that a reveal it asked for before the list
    /// is waiting.
    private(set) var revealsWaitingForFirstList = 0
    /// The last change asked for, which the next one waits on so the changes
    /// reach core in the order they were made.
    @ObservationIgnored
    private var changeTask: Task<Void, Never>?
    /// Set when a read failed. The reads behind a failed one are finished,
    /// so the next change has to build a new list rather than read under
    /// the list whose reads have ended.
    @ObservationIgnored
    private var sourceFailed = false

    /// Why the list could not be read, for as long as that stands.
    ///
    /// A read that fails delivers no rows, no watched folders and no summary,
    /// which is indistinguishable from a library that has none, so the import
    /// tab must render this rather than its "add a folder" prompt. Nil is the
    /// only state in which the absence of folders means the person has not
    /// added any.
    private(set) var loadFailure: DisplayError?

    init(
        importStore: ImportStore,
        uiStore: UiStore,
        selection: ImportSelection,
        defaults: UserDefaults = .standard,
        onSelection:
            @escaping @MainActor @Sendable (BridgeSelectionSummary) -> Void = {
                _ in
            },
        makeHandle: @escaping () async throws -> ImportListHandle
    ) {
        self.importStore = importStore
        self.uiStore = uiStore
        self.selection = selection
        selection.onError = { [uiStore] in uiStore.showError($0) }
        self.onSelection = onSelection
        self.makeHandle = makeHandle
        self.defaults = defaults
        // A saved order that doesn't decode is logged and sorts newest first.
        let initialOrder =
            defaults.decoded(
                BridgeImportListOrder.self,
                forKey: Self.sortPreferenceKey
            ) ?? .newestFirst
        sortOrder = initialOrder
        view = BridgeImportListView(
            tab: uiStore.importCandidateTab,
            filterText: uiStore.importCandidateFilterText,
            pendingFilter: uiStore.importCandidatePendingFilter,
            order: initialOrder
        )
    }

    /// The request for the view and folds the person asked for.
    private var asked: BridgeImportListRequest {
        BridgeImportListRequest(view: view, folds: folds)
    }

    /// Build the list and read its first rows. Called once the app's
    /// subscriptions start, and again when a change follows a failed read.
    func startLoad() {
        reloadTask?.cancel()
        reloadTask = Task { [weak self] in
            await self?.reload()
        }
    }

    func setTab(_ tab: BridgeTriageTab) {
        uiStore.setImportCandidateTab(tab)
        updateView { $0.tab = tab }
    }

    func setFilterText(_ text: String) {
        uiStore.setImportCandidateFilterText(text)
        updateView { $0.filterText = text }
    }

    /// Narrow Found's rows to one filter entry.
    func setPendingFilter(_ filter: BridgePendingFilter) {
        uiStore.setImportCandidatePendingFilter(filter)
        updateView { $0.pendingFilter = filter }
    }

    func setSortOrder(_ order: BridgeImportListOrder) {
        sortOrder = order
        defaults.setEncoded(order, forKey: Self.sortPreferenceKey)
        updateView { $0.order = order }
    }

    /// The list on screen.
    var list: PaginatedList<BridgeImportListItem>? { current?.list }

    /// The request the list on screen reads, which the filter field's count
    /// is checked against.
    var shown: BridgeImportListRequest? { current?.request }

    /// How many of the tab's rows the list shows, when the list on screen
    /// was read under `narrowing`, the filter the field holds now; nil when
    /// the list was read for another filter or has not counted yet, so the
    /// field shows a count only next to the filter core counted it for.
    func narrowedCount(
        under narrowing: ImportListNarrowing
    ) -> BridgeNarrowedCount? {
        guard let current,
            narrowing == ImportListNarrowing(of: current.request.view)
        else { return nil }
        return current.list.narrowed
    }

    /// Open the list at `position`: the person's place is its row, at its
    /// top, and the table shown scrolls there.
    private func open(at position: Int) {
        place = ImportListPlace(row: .atPosition, position: position, offset: 0)
        openings.send(position)
    }

    /// How many rows each tab holds, which core sends with the list's count.
    /// The value is `nil` until core sends the count.
    var tabCounts: BridgeTriageTabCounts? {
        list?.importCount?.tabs
    }

}

// MARK: - Revealing, changes and loading

extension ImportListSlot {
    /// Navigate to the first candidate the identification count is still
    /// waiting on, as core finds it when asked. `nil` when nothing is.
    /// Asked while the slot reads its first list, it waits for that list.
    func revealFirstIdentifying() async throws
        -> (candidateKey: Data, position: Int)?
    {
        let current = try await firstShown()
        guard
            let candidateKey = try await current.handle
                .firstIdentifyingCandidate(sortOrder),
            let position = try await revealCandidate(candidateKey)
        else { return nil }
        return (candidateKey, position)
    }

    /// The list on screen. A reveal asked for while the slot reads its first
    /// list waits for that read, so the reveal lands once the list does.
    /// Throws `ImportListSlotError.noList` when no read is running, or the
    /// read showed no list.
    private func firstShown() async throws -> ShownImportList {
        if current == nil {
            revealsWaitingForFirstList += 1
            defer { revealsWaitingForFirstList -= 1 }
            await reloadTask?.value
        }
        guard let current else { throw ImportListSlotError.noList }
        return current
    }

    /// Navigate to the candidate's current authoritative placement, even when
    /// that placement has moved it out of the list presently on screen, and
    /// select it alone. Core answers the request that reveals it from the
    /// list the person asked for, and where it sits there; the slot shows
    /// that request. Returns that position; `nil` when no tab lists the
    /// candidate. Asked while the slot reads its first list, the reveal
    /// waits for that list; with no list to show it in, it throws
    /// `ImportListSlotError.noList`. Cancelling the caller cancels the
    /// reveal: until the new list is on screen, the slot changes neither the
    /// view nor the selection.
    func revealCandidate(_ candidateKey: Data) async throws -> Int? {
        let reveal = enqueue {
            [self] () async -> Result<Int?, any Error> in
            do {
                let handle = try await firstShown().handle
                guard
                    let reveal = try await handle.revealCandidate(
                        asked,
                        candidateKey
                    ),
                    !Task.isCancelled
                else { return .success(nil) }
                let position = Int(reveal.position)
                guard
                    try await show(
                        reveal.request,
                        at: position,
                        through: handle
                    )
                else { return .success(nil) }
                ask(reveal.request)
                selectOnly(candidateKey)
                return .success(position)
            }
            catch { return .failure(error) }
        }
        let outcome = await withTaskCancellationHandler {
            await reveal.value
        } onCancel: {
            reveal.cancel()
        }
        return try outcome.get()
    }

    /// Make `request` the view and folds the person asked for, and the
    /// session state the sidebar renders from.
    private func ask(_ request: BridgeImportListRequest) {
        uiStore.setImportCandidateTab(request.view.tab)
        uiStore.setImportCandidateFilterText(request.view.filterText)
        uiStore.setImportCandidatePendingFilter(request.view.pendingFilter)
        view = request.view
        folds = request.folds
    }

    private func updateView(_ change: (inout BridgeImportListView) -> Void) {
        var next = view
        change(&next)
        guard next != view else { return }
        view = next
        // A failed read is not retried on its own: it is reported, the list
        // shows what it could not load, and the next thing the person does
        // with the list is what asks core again.
        if sourceFailed {
            startLoad()
            return
        }
        guard current != nil else { return }
        enqueue { [self] in await reconcile() }
    }

    /// Run `work` after the changes asked for before it have finished, so
    /// core gets them in the order the person made them.
    @discardableResult
    private func enqueue<Value: Sendable>(
        _ work: @escaping @MainActor () async -> Value
    ) -> Task<Value, Never> {
        let previous = changeTask
        let task = Task { @MainActor in
            await previous?.value
            return await work()
        }
        changeTask = Task { @MainActor in _ = await task.value }
        return task
    }

    /// Show the list under the view and folds asked for now, unless it is
    /// the list on screen. Core decides what the change comes to: where the
    /// new list opens and which selected rows leave the selection.
    private func reconcile() async {
        let next = asked
        guard let current, current.request != next else { return }
        let handle = current.handle
        do {
            let generation = selection.generation
            let change = try await handle.viewChange(
                before: current.request,
                after: next,
                selected: selection.keys.inByteOrder
            )
            if selection.generation == generation {
                selection.remove(change.left)
            }
            switch change.opening {
            case .top:
                try await show(next, at: 0, through: handle, opening: true)
            case .at(let position):
                try await show(
                    next,
                    at: Int(position),
                    through: handle,
                    opening: true
                )
            case .inPlace:
                // The row on top stays on top: the new list opens where the
                // list still on screen has it, and the table puts the release
                // that was on top back on top.
                try await show(
                    next,
                    at: current.list.topPosition,
                    through: handle
                )
            }
        }
        catch {
            fail(error)
        }
    }

    /// Read `request` through `handle` in a new list opened at `position`,
    /// and show it in place of the list on screen once it has its first rows
    /// and its count: until then the sidebar keeps drawing the list it has.
    /// With `opening`, the table scrolls to `position`. Answers whether the
    /// new list is on screen: a cancelled or failed read leaves the old one.
    @discardableResult
    private func show(
        _ request: BridgeImportListRequest,
        at position: Int,
        through handle: ImportListHandle,
        opening: Bool = false
    ) async throws -> Bool {
        let next = PaginatedList<BridgeImportListItem>(
            queries: handle.queries(request),
            onError: { [weak self] error in self?.fail(error) }
        )
        await next.reopen(at: position)
        await next.waitForCount()
        guard !Task.isCancelled else {
            next.cancel()
            return false
        }
        if let failed = next.initialLoadError {
            next.cancel()
            sourceFailed = true
            loadFailure = failed
            uiStore.showError(failed)
            return false
        }
        let previous = current?.list
        current = ShownImportList(handle: handle, list: next, request: request)
        selection.show(request)
        previous?.cancel()
        if opening { open(at: position) }
        return true
    }

    /// A read failed: report it, and have the next change rebuild the list.
    private func fail(_ error: any Error) {
        sourceFailed = true
        // A cancellation has no line and is not a failed read.
        guard let displayed = DisplayError(error) else { return }
        loadFailure = displayed
        uiStore.showError(displayed)
    }

    private func reload() async {
        sourceFailed = false
        loadFailure = nil
        let requested = asked
        let made: ImportListHandle
        do {
            made = try await makeHandle()
        }
        catch {
            fail(error)
            return
        }
        guard !Task.isCancelled else { return }
        current?.list.cancel()
        let newList = PaginatedList<BridgeImportListItem>(
            queries: made.queries(requested),
            onError: { [weak self] error in self?.fail(error) }
        )
        openSurroundings(of: made)
        await newList.loadInitial()
        // The first read's failure never reaches `onError`: `PaginatedList`
        // keeps it as `initialLoadError` for a list view to render inline, the
        // way every other list surface reads it. The import tab is not a bare
        // list (it decides between three panes before one is drawn), so it has
        // to read that outcome here, or a library nobody could look at renders
        // as a library with no folders.
        if let initial = newList.initialLoadError {
            sourceFailed = true
            loadFailure = initial
            uiStore.showError(initial)
        }
        guard !Task.isCancelled else { return }
        current = ShownImportList(
            handle: made,
            list: newList,
            request: requested
        )
        selection.show(requested)
        // A list built anew opens at its top.
        open(at: 0)
        // The list was read under the request asked for when it was built;
        // one asked for since is asked for now.
        enqueue { [self] in await reconcile() }
    }

    /// Open the reads that follow what surrounds the list; they end with the
    /// library. The selection starts empty on the list.
    private func openSurroundings(of handle: ImportListHandle) {
        let importStore = importStore
        let selectionQuery = handle.selection()
        selection.attach(selectionQuery)
        let onSelection = onSelection
        let failed: @MainActor @Sendable (any Error) -> Void = {
            [weak self] error in
            self?.fail(error)
        }
        surroundings = [
            ScreenSubscription(
                handle.summary(),
                onValue: { importStore.applySummary($0) },
                onError: failed
            ),
            ScreenSubscription(
                handle.filterEntries(),
                onValue: { importStore.applyPendingFilterEntries($0) },
                onError: failed
            ),
            ScreenSubscription(
                selectionQuery.values,
                onValue: { onSelection($0) },
                onError: failed
            ),
        ]
    }
}

#if DEBUG
    extension ImportListSlot {
        // periphery:ignore - called by baeTests, through the fixtures' `preview`
        /// A slot over a fixed set of items, with `tabCounts` as the import
        /// list's count holds them. This function loads the items into the
        /// list before it returns, so a canvas draws them without a live
        /// query.
        static func seededWithItems(
            importStore: ImportStore,
            uiStore: UiStore,
            items: [BridgeImportListItem],
            tabCounts: BridgeTriageTabCounts,
            selection: ImportSelection
        ) -> ImportListSlot {
            let tab = uiStore.importCandidateTab
            let extra = BridgeListCountExtra.import(
                narrowed: nil,
                tabs: tabCounts
            )
            let handle = ImportListHandle.fixed(
                items: items,
                tab: tab,
                extra: extra
            )
            let slot = ImportListSlot(
                importStore: importStore,
                uiStore: uiStore,
                selection: selection,
                makeHandle: { handle }
            )
            let list = PaginatedList<BridgeImportListItem>(
                queries: handle.queries(slot.asked),
                onError: { _ in }
            )
            list.preloadRows(rows: items, extra: extra)
            slot.current = ShownImportList(
                handle: handle,
                list: list,
                request: slot.asked
            )
            selection.show(slot.asked)
            return slot
        }
    }
#endif

// MARK: - Folds

extension ImportListSlot {
    /// Fold one group open or shut. Its entries leave the list when it folds,
    /// so this changes what every later position holds.
    func setGroupExpanded(
        _ group: BridgeFolderReleaseDecisionKey,
        _ expanded: Bool
    ) {
        foldGroups { $0.setting(group, folded: !expanded) }
    }

    /// Fold every group open or shut as one change.
    func setAllGroupsExpanded(_ expanded: Bool) {
        foldGroups { _ in expanded ? .allOpen : .allFolded }
    }

    /// Change the folded groups to what `fold` makes of them, and show the
    /// list under them once the changes asked for before have finished, so
    /// core gets them in the order the person made them. The slot rebuilds a
    /// list whose read failed instead, under the new folds.
    private func foldGroups(
        _ fold: (BridgeImportListFolds) -> BridgeImportListFolds
    ) {
        folds = fold(folds)
        if sourceFailed {
            startLoad()
            return
        }
        guard current != nil else { return }
        enqueue { [self] in await reconcile() }
    }
}

// MARK: - Place

extension ImportListSlot {
    /// Where the candidate on top of `place` sits now in the list the view
    /// shows, as core places it under the view's filters and folds: `nil`
    /// when the place's row is not a candidate's, or the view does not show
    /// the candidate. The slot reports a failed read and answers `nil`.
    func position(ofPlace place: ImportListPlace) async -> Int? {
        guard case .candidate(let candidateKey) = place.row, let current
        else { return nil }
        do {
            return try await current.handle
                .candidatePosition(
                    current.request,
                    candidateKey
                )
                .map(Int.init)
        }
        catch {
            uiStore.showError(error)
            return nil
        }
    }
}

// MARK: - Selection

extension ImportListSlot {
    /// Make `candidateKey` the whole selection.
    func selectOnly(_ candidateKey: Data) {
        selection.select([candidateKey])
    }

    /// Make `edit`, the change a click or key made, to the selection. Core
    /// reads the rows a Shift-click adds in the list the slot shows.
    func editSelection(_ edit: ImportListSelectionRequest.Edit) {
        selection.edit(edit)
    }
}

extension PaginatedList<BridgeImportListItem> {
    /// Whether the text filter or the filter entry narrows the list, as the
    /// list's count says; `false` until core delivers the count.
    var isNarrowed: Bool {
        narrowed != nil
    }

    /// How many of the tab's rows the list shows while its filters narrow
    /// it, as the list's count says; `nil` while they narrow nothing, or
    /// until core delivers the count.
    fileprivate var narrowed: BridgeNarrowedCount? {
        importCount?.narrowed
    }

    /// What core sends with the import list's count beside its rows: how
    /// many rows the filters keep, and how many rows each tab holds. The
    /// value is `nil` until core delivers the count. Core sends an import
    /// list's count with the import kind of extra only, so any other kind
    /// stops the app.
    fileprivate var importCount:
        (narrowed: BridgeNarrowedCount?, tabs: BridgeTriageTabCounts)?
    {
        guard let count else { return nil }
        guard case .import(let narrowed, let tabs) = count.extra else {
            preconditionFailure(
                "the import list's count carried \(count.extra)"
            )
        }
        return (narrowed, tabs)
    }
}

extension BridgeImportListFolds {
    /// Every group open.
    static let allOpen = BridgeImportListFolds.only(groups: [])
    /// Every group folded shut, the groups Found gains later included.
    static let allFolded = BridgeImportListFolds.allBut(groups: [])

    /// These folds with `group` folded shut, or open.
    func setting(
        _ group: BridgeFolderReleaseDecisionKey,
        folded: Bool
    ) -> BridgeImportListFolds {
        switch self {
        case .only(let groups):
            .only(groups: Set(groups).holding(group, folded).inKeyOrder)
        case .allBut(let groups):
            .allBut(groups: Set(groups).holding(group, !folded).inKeyOrder)
        }
    }
}

extension Set<BridgeFolderReleaseDecisionKey> {
    /// This set with `key` in it, or without it.
    fileprivate func holding(
        _ key: BridgeFolderReleaseDecisionKey,
        _ held: Bool
    ) -> Self {
        held ? union([key]) : subtracting([key])
    }

    /// The keys in the order of their bytes, so a request holds the same
    /// folds in one order.
    var inKeyOrder: [BridgeFolderReleaseDecisionKey] {
        sorted { first, second in
            if first.watchedFolderPath != second.watchedFolderPath {
                return first.watchedFolderPath.lexicographicallyPrecedes(
                    second.watchedFolderPath
                )
            }
            return first.relativeFolder.lexicographicallyPrecedes(
                second.relativeFolder
            )
        }
    }
}
