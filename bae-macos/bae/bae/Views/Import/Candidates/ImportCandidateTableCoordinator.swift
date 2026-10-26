import AppKit
import BaeKit
import Combine
import SwiftUI

/// The import list table's rows, scrolling and page holds. Its selection
/// handling is in `ImportCandidateTableCoordinator+Selection.swift`.
@MainActor
final class ImportCandidateTableCoordinator: NSObject, NSTableViewDataSource,
    NSTableViewDelegate
{
    private(set) var list: PaginatedList<BridgeImportListItem>
    let importStore: ImportStore
    let listSlot: ImportListSlot
    var actions: ImportListEntryActions
    var environment: ImportListRowEnvironment
    var onPersonSelected: () -> Void
    private(set) weak var tableView: ImportCandidateTableView?
    private weak var scrollView: NSScrollView?
    /// Reveals, and the clip view's and scroll view's notifications.
    private var subscriptions: [AnyCancellable] = []

    /// The rows of `list` the table was last loaded with.
    private var shownRows: ImportCandidateTableRows?
    /// The appearance the table's rows were last drawn in.
    private var drawnAppearance: HostedAppearance?
    /// The content the table last took, which a new delivery is read against.
    private var content: ImportCandidateTableContent?
    /// Each row in view's hold on its page, by row.
    private var rowHolds: [Int: Task<Void, Never>] = [:]
    /// The key of the row the person last pointed at without Shift.
    var anchorKey: String?
    /// Set while an update moves the rows and scrolls to keep the place, so
    /// the scrolling it does is not taken for the person's.
    private var updating = false
    /// The place a table mounted anew goes back to, until it has gone there.
    private var pendingPlace: ImportListPlace?
    /// Core's answer for where the pending place's row is now.
    private var placeTask: Task<Void, Never>?

    init(
        list: PaginatedList<BridgeImportListItem>,
        importStore: ImportStore,
        listSlot: ImportListSlot,
        actions: ImportListEntryActions,
        environment: ImportListRowEnvironment,
        onPersonSelected: @escaping () -> Void
    ) {
        self.list = list
        self.importStore = importStore
        self.listSlot = listSlot
        self.actions = actions
        self.environment = environment
        self.onPersonSelected = onPersonSelected
        drawnAppearance = environment.appearance
    }

    // MARK: Lifecycle

    /// Drive `tableView`, scrolled by `scrollView`: go back to the place the
    /// person left the list at once the table is laid out, and scroll to each
    /// position `reveals` asks to see.
    func attach(
        _ tableView: ImportCandidateTableView,
        in scrollView: NSScrollView,
        reveals: AnyPublisher<Int, Never>
    ) {
        self.tableView = tableView
        self.scrollView = scrollView
        tableView.dataSource = self
        tableView.delegate = self
        tableView.target = self
        tableView.doubleAction = #selector(openClickedRow(_:))
        tableView.onSelectAll = { [weak self] in self?.selectAllShown() }
        pendingPlace = listSlot.place
        let clipView = scrollView.contentView
        clipView.postsBoundsChangedNotifications = true
        clipView.postsFrameChangedNotifications = true
        let center = NotificationCenter.default
        subscriptions = [
            reveals.sink { [weak self] position in
                MainActor.assumeIsolated { self?.showCentred(position) }
            },
            center.publisher(
                for: NSView.boundsDidChangeNotification,
                object: clipView
            )
            .sink { [weak self] _ in
                MainActor.assumeIsolated { self?.clipViewScrolled() }
            },
            center.publisher(
                for: NSView.frameDidChangeNotification,
                object: clipView
            )
            .sink { [weak self] _ in
                MainActor.assumeIsolated { self?.clipViewResized() }
            },
            center.publisher(
                for: NSScrollView.willStartLiveScrollNotification,
                object: scrollView
            )
            .sink { [weak self] _ in
                MainActor.assumeIsolated { self?.endPendingPlace() }
            },
        ]
    }

    /// The table is going away: its holds, its wait for core and what it
    /// listens to end.
    func detach() {
        releaseHolds()
        placeTask?.cancel()
        placeTask = nil
        subscriptions = []
    }

    // MARK: Updates

    /// Take what the list holds now: reload the rows when they changed,
    /// scroll as the content change asks, highlight the rows `shownSelected`
    /// names, and hold the pages of the rows in view.
    func update(
        list: PaginatedList<BridgeImportListItem>,
        rows: ImportCandidateTableRows,
        read: ImportCandidateListContentRead,
        shownSelected: Set<String>,
        revealing: Bool
    ) {
        updating = true
        defer {
            updating = false
            recordPlace()
        }
        if list !== self.list {
            // The rows held the replaced list's pages, a rebuilt list's
            // selection starts empty, and its rows are loaded anew.
            releaseHolds()
            anchorKey = nil
            shownRows = nil
            self.list = list
        }
        if revealing {
            endPendingPlace()
        }
        load(rows)
        if environment.appearance != drawnAppearance {
            drawnAppearance = environment.appearance
            tableView?.reloadData()
        }
        scroll(for: read, revealing: revealing)
        highlight(shownSelected)
        holdRowsInView()
    }

    /// Load the table with `rows` when they are not the ones it shows.
    private func load(_ rows: ImportCandidateTableRows) {
        guard rows != shownRows else { return }
        shownRows = rows
        tableView?.reloadData()
    }

    /// The rows of the list as it stands, whatever the last update loaded:
    /// a reveal's position may come before the update that loads its rows.
    private func loadCurrentRows() {
        load(
            ImportCandidateTableRows(
                count: list.totalCount,
                revision: list.contentRevision
            )
        )
    }

    private func scroll(
        for read: ImportCandidateListContentRead,
        revealing: Bool
    ) {
        guard var content else {
            // The first delivery a table takes is what it opens on: at the
            // place the person left the list, or, when the list was narrowed
            // anew since, at its first selected row or its top.
            content = ImportCandidateTableContent(read)
            if let place = pendingPlace, place.narrowing != read.narrowing {
                pendingPlace = ImportListPlace(
                    row: .unloaded,
                    position: read.firstSelected ?? 0,
                    offset: 0,
                    narrowing: read.narrowing
                )
            }
            return
        }
        let scrolling = content.take(read, revealing: revealing)
        self.content = content
        switch scrolling {
        case .showOnTop(let row):
            endPendingPlace()
            anchorKey = nil
            scroll(toTop: row, offset: 0)
        case .keepPlace:
            guard pendingPlace == nil, let place = listSlot.place,
                let position = place.loadedPosition(in: list)
            else { return }
            scroll(toTop: position, offset: place.offset)
        case nil:
            break
        }
    }

    // MARK: Rows

    func numberOfRows(in _: NSTableView) -> Int {
        rowCount
    }

    /// How many rows the table was last loaded with.
    var rowCount: Int {
        shownRows?.count ?? 0
    }

    /// The entry at `row`, once its page has landed.
    func item(at row: Int) -> BridgeImportListItem? {
        list.idAt(row).flatMap { importStore.items[$0] }
    }

    /// Every row one height: a group row would otherwise take the shorter
    /// height the table gives its group rows.
    func tableView(_: NSTableView, heightOfRow _: Int) -> CGFloat {
        ImportCandidateTableLayout.rowHeight
    }

    func tableView(_: NSTableView, isGroupRow row: Int) -> Bool {
        if case .groupHeader = item(at: row) { return true }
        return false
    }

    func tableView(_: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        guard case .groupHeader = item(at: row) else { return nil }
        return ImportListGroupRowView()
    }

    func tableView(
        _ tableView: NSTableView,
        viewFor _: NSTableColumn?,
        row: Int
    ) -> NSView? {
        guard let item = item(at: row) else {
            // A row drawn before its page lands asks for the page, and the
            // page's delivery reloads the table. The rows in view hold their
            // pages already; this is for a row off screen that accessibility
            // reads, which the table draws for it.
            let list = list
            Task { await list.withPage(containing: row) {} }
            return reused(tableView, Self.placeholder) { _ in
                NSTableCellView()
            }
        }
        let cell = reused(tableView, Self.identifier(for: item)) {
            ImportListEntryCell(identifier: $0)
        }
        cell.show(
            ImportListEntry(
                item: item,
                coverContent: item.triageRow.flatMap {
                    importStore.sidebarCover(for: $0)
                },
                gapAbove: row > 0,
                actions: actions,
                environment: environment
            )
        )
        return cell
    }

    private static let placeholder = NSUserInterfaceItemIdentifier(
        "placeholder"
    )

    /// One reuse pool per kind of row, so a reused cell keeps the shape of
    /// the row it draws.
    private static func identifier(
        for item: BridgeImportListItem
    ) -> NSUserInterfaceItemIdentifier {
        switch item {
        case .groupHeader: NSUserInterfaceItemIdentifier("header")
        case .candidate: NSUserInterfaceItemIdentifier("candidate")
        case .invalid: NSUserInterfaceItemIdentifier("invalid")
        }
    }

    private func reused<Cell: NSView>(
        _ tableView: NSTableView,
        _ identifier: NSUserInterfaceItemIdentifier,
        make: (NSUserInterfaceItemIdentifier) -> Cell
    ) -> Cell {
        if let cell = tableView.makeView(withIdentifier: identifier, owner: nil)
            as? Cell
        {
            return cell
        }
        let cell = make(identifier)
        cell.identifier = identifier
        return cell
    }

    func tableView(
        _: NSTableView,
        typeSelectStringFor _: NSTableColumn?,
        row: Int
    ) -> String? {
        item(at: row)?.typeSelectText
    }

    /// A double-click on a Done row opens the release imported from it.
    @objc
    private func openClickedRow(_ sender: NSTableView) {
        guard sender.clickedRow >= 0,
            let release = item(at: sender.clickedRow)?.triageRow?
                .importedRelease
        else { return }
        actions.onShowInLibrary(release)
    }
}

// MARK: - Page holds

extension ImportCandidateTableCoordinator {
    /// Each row in view holds its page, and so does the row on either side
    /// of the view, which an arrow key moves to; a row leaving lets go.
    private func holdRowsInView() {
        let count = rowCount
        guard let tableView, count > 0 else {
            releaseHolds()
            return
        }
        let range = tableView.rows(in: tableView.visibleRect)
        let lower = max(range.location - 1, 0)
        let upper = min(range.location + range.length + 1, count)
        let inView = lower < upper ? Set(lower..<upper) : []
        for (row, hold) in rowHolds where !inView.contains(row) {
            hold.cancel()
            rowHolds.removeValue(forKey: row)
        }
        let list = list
        for row in inView where rowHolds[row] == nil {
            rowHolds[row] = Task { await list.holdPage(containing: row) }
        }
    }

    private func releaseHolds() {
        for hold in rowHolds.values {
            hold.cancel()
        }
        rowHolds.removeAll()
    }
}

// MARK: - Scrolling and place

extension ImportCandidateTableCoordinator {
    private func clipViewScrolled() {
        holdRowsInView()
        recordPlace()
    }

    private func clipViewResized() {
        holdRowsInView()
        if let pendingPlace, placeTask == nil,
            let clipView = scrollView?.contentView, clipView.bounds.height > 0
        {
            goBack(to: pendingPlace)
        }
        else {
            recordPlace()
        }
    }

    /// Go back to `place` in a table mounted anew: to its row where the list
    /// holds it, else to where the row was while core says where it is now.
    private func goBack(to place: ImportListPlace) {
        if let position = place.loadedPosition(in: list) {
            scroll(toTop: position, offset: place.offset)
            endPendingPlace()
            return
        }
        scroll(toTop: place.position, offset: place.offset)
        guard case .candidate = place.row else {
            endPendingPlace()
            return
        }
        let listSlot = listSlot
        placeTask = Task { [weak self] in
            let located = await listSlot.position(ofPlace: place)
            guard let self, !Task.isCancelled, pendingPlace == place else {
                return
            }
            placeTask = nil
            if let located {
                loadCurrentRows()
                scroll(toTop: located, offset: place.offset)
            }
            endPendingPlace()
        }
    }

    /// Stop going back to the pending place — the table is there, or the
    /// person or a reveal moved it — and take where it is as the place.
    func endPendingPlace() {
        placeTask?.cancel()
        placeTask = nil
        guard pendingPlace != nil else { return }
        pendingPlace = nil
        recordPlace()
    }

    /// Take the row on top of the table, and how far into it the table is
    /// scrolled, as the person's place.
    private func recordPlace() {
        guard !updating, pendingPlace == nil, let tableView,
            let clipView = scrollView?.contentView, rowCount > 0,
            let content
        else { return }
        let top = clipView.bounds.minY
        // The row under the top edge, not the header floating over it.
        let row = min(
            Int(max(top, 0) / ImportCandidateTableLayout.rowHeight),
            rowCount - 1
        )
        listSlot.place = ImportListPlace(
            row: placeRow(at: row),
            position: row,
            offset: top - tableView.rect(ofRow: row).minY,
            narrowing: content.narrowing
        )
    }

    private func placeRow(at row: Int) -> ImportListPlace.Row {
        guard let stableKey = list.idAt(row) else { return .unloaded }
        if let candidate = importStore.items[stableKey]?.triageRow {
            return .candidate(
                stableKey: stableKey,
                candidateKey: candidate.candidateKey
            )
        }
        return .entry(stableKey: stableKey)
    }

    /// Put `row` at the top of the table, scrolled `offset` into it.
    private func scroll(toTop row: Int, offset: CGFloat) {
        guard let tableView, row >= 0, row < tableView.numberOfRows else {
            return
        }
        scroll(toY: tableView.rect(ofRow: row).minY + offset)
    }

    /// Show `row` in the middle of the table: a reveal's row.
    private func showCentred(_ row: Int) {
        endPendingPlace()
        loadCurrentRows()
        guard let tableView, let clipView = scrollView?.contentView,
            row >= 0, row < tableView.numberOfRows
        else { return }
        let height = clipView.bounds.height
        scroll(toY: tableView.rect(ofRow: row).midY - height / 2)
    }

    private func scroll(toY y: CGFloat) {
        guard let tableView, let scrollView else { return }
        let clipView = scrollView.contentView
        let highest = max(tableView.bounds.height - clipView.bounds.height, 0)
        let origin = NSPoint(x: 0, y: min(max(y, 0), highest))
        guard origin != clipView.bounds.origin else { return }
        clipView.scroll(to: origin)
        scrollView.reflectScrolledClipView(clipView)
    }
}

extension BridgeImportListItem {
    /// The candidate's row, for a candidate's entry.
    var triageRow: BridgeTriageRow? {
        if case .candidate(_, let row, _) = self { return row }
        return nil
    }
}
