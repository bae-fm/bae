import BaeKit
import Foundation
import Observation

/// The import sidebar's list machinery: the warm `PaginatedList`, the page
/// source behind it, and the view that source is showing.
///
/// The view — which tab, which filter text, which groups are folded shut —
/// decides which items sit at which offsets, so it travels with the request
/// rather than being applied to a page after it arrives. Changing it therefore
/// reconfigures the one source instead of building a new list: the list
/// re-reads the pages it holds when the reconfigured value arrives.
///
/// `UiStore` keeps the tab, the filter text and the disclosure state as
/// session state, so every setter here writes both: the store the sidebar
/// renders from, and the request core answers.
@MainActor
@Observable
final class ImportListSlot {
    private(set) var list: PaginatedList<BridgeImportListItem>?

    @ObservationIgnored
    private var view: BridgeImportListView
    @ObservationIgnored
    private let makeSource: (BridgeImportListView) -> ImportListPages
    @ObservationIgnored
    private var pages: ImportListPages?
    @ObservationIgnored
    private let importStore: ImportStore
    @ObservationIgnored
    private let uiStore: UiStore
    @ObservationIgnored
    private var reloadTask: Task<Void, Never>?
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
        makeSource: @escaping (BridgeImportListView) -> ImportListPages
    ) {
        self.importStore = importStore
        self.uiStore = uiStore
        self.makeSource = makeSource
        view = BridgeImportListView(
            tab: uiStore.importCandidateTab,
            filterText: uiStore.importCandidateFilterText,
            collapsedGroups: uiStore.collapsedReleaseGroupKeys,
            // macOS offers no order control; the list reads in the order the
            // watched roots and the folder paths give it.
            order: .pathAscending
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

    /// Fold one group open or shut. Its entries leave the list when it folds,
    /// so this changes what every later offset holds.
    func setGroupExpanded(_ id: ReleaseGroupDisclosureID, _ expanded: Bool) {
        uiStore.setReleaseGroupExpanded(id, expanded)
        updateView { $0.collapsedGroups = uiStore.collapsedReleaseGroupKeys }
    }

    /// Keep disclosure state only for the groups the queue still has. A group
    /// that is gone takes its folded state with it, which can change the
    /// request.
    func retainGroups(_ keys: [BridgeFolderReleaseDecisionKey]) {
        uiStore.retainReleaseGroupDisclosureIDs(
            Set(keys.map(ReleaseGroupDisclosureID.init(key:)))
        )
        updateView { $0.collapsedGroups = uiStore.collapsedReleaseGroupKeys }
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
        pages?.setView(next)
    }

    private func reload() async {
        sourceFailed = false
        loadFailure = nil
        let pages = makeSource(view)
        let importStore = importStore
        let newList = PaginatedList<BridgeImportListItem>(
            pageSource: pages.source,
            ingest: { (items: [BridgeImportListItem]) in
                importStore.ingest(items)
            },
            onError: { [weak self] (error: any Error) in
                self?.sourceFailed = true
                // A cancellation has no line and is not a failed read.
                guard let displayed = DisplayError(error) else { return }
                self?.loadFailure = displayed
                self?.uiStore.showError(displayed)
            },
            onSnapshot: { (ids: [String], _: Int) in
                importStore.retainItems(ids)
            }
        )
        await newList.loadInitial()
        guard !Task.isCancelled else { return }
        self.pages = pages
        list = newList
    }

    #if DEBUG
        /// A slot over a fixed set of items, for previews and tests. The list
        /// is seeded synchronously so a canvas draws without a live query.
        static func preview(
            importStore: ImportStore,
            uiStore: UiStore,
            items: [BridgeImportListItem]
        ) -> ImportListSlot {
            let slot = ImportListSlot(
                importStore: importStore,
                uiStore: uiStore,
                makeSource: { _ in
                    ImportListPreviewPageSource(items: items).pages
                }
            )
            importStore.ingest(items)
            let list = PaginatedList<BridgeImportListItem>(
                pageSource: ImportListPreviewPageSource(items: items),
                ingest: { _ in },
                onError: { _ in }
            )
            list.preloadForPreview(ids: items.map(\.id))
            slot.list = list
            return slot
        }
    #endif
}
