import BaeKit
import Foundation
import OSLog
import Observation

private let importListLogger = Logger.bae("ImportListSlot")

/// The import sidebar's list machinery: the warm `PaginatedList`, the page
/// source behind it, and the view that source is showing.
///
/// The view — which tab, which filter text, which order — and the groups
/// folded shut decide which items sit at which offsets, so they are core's
/// list's rather than applied to a page after it arrives. Changing them
/// therefore changes the one source instead of building a new list: the list
/// re-reads the pages it holds when the changed value arrives. Until then the
/// rows it holds are the previous view's, which the list does not show as
/// this one's. Which groups are folded is core's alone; each header says
/// whether it is open.
///
/// `UiStore` keeps the tab and the filter text as session state, so every
/// setter of them here writes both: the store the sidebar renders from, and
/// the view core answers.
@MainActor
@Observable
final class ImportListSlot {
    private(set) var list: PaginatedList<BridgeImportListItem>?
    /// Whether the rows the list holds answer the view it shows: false from
    /// the moment another tab, filter or order is asked for until the read
    /// that reflects it has landed. Before then the list's rows are the
    /// previous view's.
    private(set) var answersView = true
    private(set) var sortOrder: BridgeImportListOrder
    /// Where the person is in the list, as the table last recorded it. Held
    /// here rather than by the table so the import tab finds it again after
    /// another tab was shown. Nothing draws from it.
    @ObservationIgnored
    var place: ImportListPlace?
    @ObservationIgnored
    private let defaults: UserDefaults
    private static let sortPreferenceKey = "importCandidateSortOrder"

    /// The view core answers the list for.
    @ObservationIgnored
    private(set) var view: BridgeImportListView
    @ObservationIgnored
    private let makeSource:
        (BridgeImportListView) async throws -> ImportListPages
    @ObservationIgnored
    private var pages: ImportListPages?
    @ObservationIgnored
    private let importStore: ImportStore
    @ObservationIgnored
    private let uiStore: UiStore
    /// What a person does to the rows goes here; core's list keeps it to the
    /// rows the view shows.
    @ObservationIgnored
    private let selection: ImportSelection
    /// The last selection write asked for, which the next one waits on.
    @ObservationIgnored
    private var selectionTask: Task<Void, Never>?
    /// The last fold asked for, which the next one waits on.
    @ObservationIgnored
    private var foldTask: Task<Void, Never>?
    @ObservationIgnored
    private var reloadTask: Task<Void, Never>?
    /// The wait for the read that answers the view last asked for.
    @ObservationIgnored
    private var viewAnswer: Task<Void, Never>?
    /// Set when a page read failed. The subscription behind a failed read is
    /// finished, so the next view change has to build a new one rather than
    /// reconfigure the dead one.
    @ObservationIgnored
    private var sourceFailed = false

    /// Why the list could not be read, for as long as that stands.
    ///
    /// A read that fails delivers no rows, no watched folders and no summary,
    /// which is indistinguishable from a library that has none — so the import
    /// tab must render this rather than its "add a folder" prompt. Nil is the
    /// only state in which the absence of folders means the person has not
    /// added any.
    private(set) var loadFailure: DisplayError?

    init(
        importStore: ImportStore,
        uiStore: UiStore,
        selection: ImportSelection,
        defaults: UserDefaults = .standard,
        makeSource:
            @escaping (BridgeImportListView) async throws -> ImportListPages
    ) {
        self.importStore = importStore
        self.uiStore = uiStore
        self.selection = selection
        self.makeSource = makeSource
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

    /// Build the list and read its first page. Called once the app's
    /// subscriptions start, and again when a view change follows a failed
    /// read.
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

    /// Navigate to the first candidate the identification count is still
    /// waiting on, as core finds it when asked. `nil` when nothing is.
    func revealFirstIdentifying() async throws
        -> (candidateKey: Data, position: Int)?
    {
        guard let pages,
            let candidateKey = try await pages.firstIdentifyingCandidate(),
            let position = try await revealCandidate(candidateKey)
        else { return nil }
        return (candidateKey, position)
    }

    /// Navigate to the candidate's current authoritative placement, even when
    /// that placement has moved it out of the list presently on screen, and
    /// select it alone.
    func revealCandidate(_ candidateKey: Data) async throws -> Int? {
        guard let pages,
            let location = try await pages.locateCandidate(candidateKey)
        else {
            return nil
        }
        uiStore.setImportCandidateTab(location.tab)
        uiStore.setImportCandidateFilterText("")
        uiStore.setImportCandidatePendingFilter(.all)
        if let groupKey = location.groupKey {
            try await pages.setGroupFolded(groupKey, false)
        }
        var next = view
        next.tab = location.tab
        next.filterText = ""
        next.pendingFilter = .all
        view = next
        // The list keeps only the rows its view shows selected, so it is
        // asked for the view that shows the candidate before it selects it.
        show(next, on: pages)
        try await selectOnly(candidateKey)
        guard let list else { return nil }
        try await pages.waitForView(next)
        let position = Int(location.visiblePosition)
        let placed = await list.holds(location.stableKey, at: position)
        guard !Task.isCancelled, placed else { return nil }
        return position
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
            answersView = false
            startLoad()
            return
        }
        guard let pages else { return }
        show(next, on: pages)
    }

    /// Ask core's list for `next`, and take its rows as the view's once the
    /// read that reflects it lands, unless another view was asked for since.
    private func show(_ next: BridgeImportListView, on pages: ImportListPages) {
        answersView = false
        pages.setView(next)
        viewAnswer?.cancel()
        viewAnswer = Task { [weak self] in
            // A failed write or read fails the list's pages, which report it.
            guard (try? await pages.waitForView(next)) != nil,
                let self, view == next
            else { return }
            answersView = true
        }
    }

    private func reload() async {
        sourceFailed = false
        loadFailure = nil
        let requested = view
        let pages: ImportListPages
        do {
            pages = try await makeSource(requested)
        }
        catch {
            sourceFailed = true
            // A cancellation has no line and is not a failed read.
            guard let displayed = DisplayError(error) else { return }
            loadFailure = displayed
            uiStore.showError(displayed)
            return
        }
        guard !Task.isCancelled else { return }
        let importStore = importStore
        let newList = PaginatedList<BridgeImportListItem>(
            pageSource: pages.source,
            ingest: { (items: [BridgeImportListItem]) in
                importStore.ingest(items)
            },
            onError: { [weak self] (error: any Error) in
                self?.sourceFailed = true
                // A cancellation has no line and is not a failed read.
                guard let displayed = DisplayError(error) else {
                    return
                }
                self?.loadFailure = displayed
                self?.uiStore.showError(displayed)
            },
            onSnapshot: { (ids: [String], _: Int) in
                importStore.retainItems(ids)
            }
        )
        await newList.loadInitial()
        // The first page's failure never reaches `onError`: `PaginatedList`
        // keeps it as `initialLoadError` for a list view to render inline, the
        // way every other list surface reads it. The import tab is not a bare
        // list — it decides between three panes before one is drawn — so it has
        // to read that outcome here, or a library nobody could look at renders
        // as a library with no folders.
        if let initial = newList.initialLoadError {
            sourceFailed = true
            loadFailure = initial
            uiStore.showError(initial)
        }
        guard !Task.isCancelled else { return }
        self.pages = pages
        list = newList
        // The list was read under the view asked for when it was built; one
        // asked for since is asked for now.
        if view == requested {
            answersView = true
        }
        else {
            show(view, on: pages)
        }
    }

    #if DEBUG
        /// A slot over a fixed set of items, for previews and tests. The list
        /// is seeded synchronously so a canvas draws without a live query.
        static func preview(
            importStore: ImportStore,
            uiStore: UiStore,
            items: [BridgeImportListItem],
            selection: ImportSelection = ImportSelection()
        ) -> ImportListSlot {
            let tab = uiStore.importCandidateTab
            let pages = ImportListPages(
                source: ImportListPreviewPageSource(items: items),
                setView: { _ in },
                waitForView: { _ in },
                setGroupFolded: { _, _ in },
                setAllGroupsFolded: { _ in },
                locateCandidate: { key in
                    items.firstIndex { $0.selectionKey == key }
                        .map {
                            BridgeImportCandidateListLocation(
                                stableKey: items[$0].id,
                                tab: tab,
                                groupKey: nil,
                                visiblePosition: UInt64($0)
                            )
                        }
                },
                firstIdentifyingCandidate: { nil }
            )
            let slot = ImportListSlot(
                importStore: importStore,
                uiStore: uiStore,
                selection: selection,
                makeSource: { _ in pages }
            )
            importStore.ingest(items)
            importStore.applyPendingFilterEntries(
                PreviewData.pendingFilterEntries()
            )
            let list = PaginatedList<BridgeImportListItem>(
                pageSource: ImportListPreviewPageSource(items: items),
                onError: { _ in }
            )
            list.preloadForPreview(rows: items)
            slot.list = list
            slot.pages = pages
            return slot
        }
    #endif
}

// MARK: - Folds

extension ImportListSlot {
    /// Fold one group open or shut. Its entries leave the list when it folds,
    /// so this changes what every later offset holds.
    func setGroupExpanded(
        _ group: BridgeFolderReleaseDecisionKey,
        _ expanded: Bool
    ) {
        foldGroups { try await $0.setGroupFolded(group, !expanded) }
    }

    /// Fold every group open or shut as one change.
    func setAllGroupsExpanded(_ expanded: Bool) {
        foldGroups { try await $0.setAllGroupsFolded(!expanded) }
    }

    /// Send a fold to core's list once the folds asked for before it have
    /// landed, so they land in the order the person made them; a failure is
    /// reported. A list whose read failed is rebuilt instead, and the rebuilt
    /// list starts with no group folded.
    private func foldGroups(
        _ fold: @escaping @MainActor (ImportListPages) async throws -> Void
    ) {
        if sourceFailed {
            startLoad()
            return
        }
        guard let pages else { return }
        let previous = foldTask
        foldTask = Task { [uiStore] in
            await previous?.value
            do {
                try await fold(pages)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}

// MARK: - Place

extension ImportListSlot {
    /// Where the candidate on top of `place` sits now in the list the view
    /// shows, as core places it: `nil` when the place's row is not a
    /// candidate's, or core places it where this view does not show it — on
    /// another tab, or where the view's filters would put it elsewhere. A
    /// failed read is reported, and answers `nil`.
    func position(ofPlace place: ImportListPlace) async -> Int? {
        guard case .candidate(let stableKey, let candidateKey) = place.row,
            let pages
        else { return nil }
        do {
            guard let location = try await pages.locateCandidate(candidateKey),
                let list
            else { return nil }
            let position = Int(location.visiblePosition)
            let placed = await list.holds(stableKey, at: position)
            guard !Task.isCancelled, placed else { return nil }
            return position
        }
        catch {
            uiStore.showError(error)
            return nil
        }
    }
}

// MARK: - Selection

extension ImportListSlot {
    /// Apply what a person did to the list's rows: `change` goes to core, and
    /// `shown` are the loaded rows the list shows selected until core's list
    /// reflects the write.
    func changeSelection(
        _ change: BridgeSelectionChange,
        showing shown: Set<Data>
    ) {
        let click = importStore.beginSelectionClick(shown)
        writeSelection { [importStore] selection in
            do {
                let revision = try await selection.change(change)
                importStore.selectionClickWritten(click, revision: revision)
            }
            catch {
                importStore.selectionClickFailed(click)
                throw error
            }
        }
    }

    /// Make `candidateKey` the whole selection, once the writes asked for
    /// before it have landed.
    func selectOnly(_ candidateKey: Data) async throws {
        try await queueSelectionWrite {
            try await $0.change(.replace(keys: [candidateKey]))
        }
        .value
    }

    /// Select every row the list shows under its view, loaded or not.
    func selectAllShown() {
        writeSelection { try await $0.selectAll() }
    }

    /// Run `write` once every selection write asked for before it has ended,
    /// so the writes land in the order the person made them; a failure is
    /// reported.
    private func writeSelection(
        _ write: @escaping @MainActor (ImportSelection) async throws -> Void
    ) {
        let task = queueSelectionWrite(write)
        Task { [uiStore] in
            if case .failure(let error) = await task.result {
                uiStore.showError(error)
            }
        }
    }

    private func queueSelectionWrite(
        _ write: @escaping @MainActor (ImportSelection) async throws -> Void
    ) -> Task<Void, any Error> {
        let previous = selectionTask
        let task = Task { [selection] in
            await previous?.value
            try await write(selection)
        }
        selectionTask = Task { _ = await task.result }
        return task
    }
}

extension PaginatedList<BridgeImportListItem> {
    /// Whether the list holds `stableKey` at `position`, once the page that
    /// covers it has answered.
    fileprivate func holds(_ stableKey: String, at position: Int) async -> Bool
    {
        await withPage(containing: position) {
            idAt(position) == stableKey
        }
    }
}
