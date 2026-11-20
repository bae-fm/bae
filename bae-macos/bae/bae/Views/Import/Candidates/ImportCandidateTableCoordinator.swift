import AppKit
import BaeKit
import Combine
import SwiftUI

/// The coordinator loads the import list table's rows, scrolls it, and holds
/// the pages of the rows in view. Its selection handling is in
/// `ImportCandidateTableCoordinator+Selection.swift`.
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
    /// Reveals, openings, and the clip view's and scroll view's
    /// notifications.
    private var subscriptions: [AnyCancellable] = []

    /// How many times a cell has been given an item to draw.
    private(set) var cellsShown = 0

    /// The rows of `list` the table was last loaded with.
    private var shownRows: ImportCandidateTableRows?
    /// The appearance the table's rows were last drawn in.
    private var drawnAppearance: HostedAppearance?
    /// Each row in view's hold on its page, by row.
    private var rowHolds: [Int: Task<Void, Never>] = [:]
    /// Each row drawn for display before its page landed asking for the
    /// page, by row, for as long as the table draws it.
    private var drawnHolds: [Int: Task<Void, Never>] = [:]
    /// The key of the row the person last pointed at without Shift.
    var anchorKey: Data?
    /// Set while an update moves the rows and scrolls to keep the place, so
    /// the scrolling it does is not taken for the person's.
    private var updating = false
    /// The place a table mounted anew goes back to, until it has gone there.
    private var pendingPlace: ImportListPlace?
    /// Core's answer for where the pending place's row is now.
    private var placeTask: Task<Void, Never>?
    /// Core's answer for where the place's row sits after the rows moved.
    /// The table records no place while it waits, because the rows moved
    /// under a table that has not scrolled. A scroll by the person ends the
    /// wait.
    private var keepTask: Task<Void, Never>?
    /// Where the header of the group on top is pinned.
    let pinnedStrip = ImportListPinnedHeaderStrip()

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
    /// person left the list at once the table is laid out, scroll to each
    /// position `reveals` asks to see, and put each row the list opens at on
    /// top.
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
        scrollView.addSubview(
            pinnedStrip,
            positioned: .above,
            relativeTo: scrollView.contentView
        )
        let clipView = scrollView.contentView
        clipView.postsBoundsChangedNotifications = true
        clipView.postsFrameChangedNotifications = true
        let center = NotificationCenter.default
        subscriptions = [
            reveals.sink { [weak self] position in
                MainActor.assumeIsolated { self?.showCentred(position) }
            },
            listSlot.openings.sink { [weak self] position in
                MainActor.assumeIsolated { self?.showOpening(position) }
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
        keepTask?.cancel()
        keepTask = nil
        subscriptions = []
    }

    // MARK: Updates

    /// Take what the list holds now: reload the rows when they changed, keep
    /// the person's place while they move, highlight the rows `shownSelected`
    /// names, and hold the pages of the rows in view.
    func update(
        list: PaginatedList<BridgeImportListItem>,
        rows: ImportCandidateTableRows,
        shownSelected: Set<Data>,
        revealing: Bool
    ) {
        updating = true
        defer {
            updating = false
            recordPlace()
        }
        if list !== self.list {
            // The rows held the replaced list's pages, a rebuilt list's
            // selection starts empty, so nothing extends from the anchor,
            // and the table loads its rows again.
            releaseHolds()
            anchorKey = nil
            shownRows = nil
            self.list = list
        }
        if revealing {
            endPendingPlace()
        }
        let rowsChanged = rows != shownRows
        load(rows)
        if environment.appearance != drawnAppearance {
            drawnAppearance = environment.appearance
            reloadRows()
        }
        keepPlace(revealing: revealing, rowsChanged: rowsChanged)
        highlight(shownSelected)
        holdRowsInView()
        refreshPinnedHeader()
    }

    /// Load the table with `rows` when they are not the ones it shows. A new
    /// list loads every row. For the same list, a change in how many rows it
    /// has is told to the table as that, and only the rows now holding
    /// something other than what their cell draws are drawn again: a row
    /// added below the view, or a value that changed a row off screen,
    /// draws no cell.
    private func load(_ rows: ImportCandidateTableRows) {
        guard rows != shownRows else { return }
        let previous = shownRows
        shownRows = rows
        guard let previous, let tableView else {
            reloadRows()
            return
        }
        if previous.count != rows.count {
            tableView.noteNumberOfRowsChanged()
        }
        redrawChangedRows(in: tableView)
        noteHeights(in: tableView)
        pinnedStrip.forget()
    }

    /// Tell the table of the rows the list's view delivered whose height in
    /// the table is not the height their row needs: a header's row is
    /// shorter than a release's, and a row's height is known only once its
    /// page has landed. A row past the view's window keeps the height the
    /// table last laid it out at until the window holds it again. The row on
    /// top stays where it is on screen as the heights above it change.
    private func noteHeights(in tableView: ImportCandidateTableView) {
        guard let delivered = list.delivered else { return }
        let end = min(delivered.start + delivered.rows.count, rowCount)
        var rows = IndexSet()
        for row in delivered.start..<max(delivered.start, end)
        where tableView.rect(ofRow: row).height
            != height(ofRow: row)
        {
            rows.insert(row)
        }
        guard !rows.isEmpty else { return }
        guard let clipView = scrollView?.contentView, let top = topRow else {
            tableView.noteHeightOfRows(withIndexesChanged: rows)
            return
        }
        let offset = tableView.rect(ofRow: top).minY - clipView.bounds.minY
        tableView.noteHeightOfRows(withIndexesChanged: rows)
        scroll(toY: tableView.rect(ofRow: top).minY - offset)
    }

    /// Draw again the rows the table has made cells for whose item is no
    /// longer the one their cell draws.
    private func redrawChangedRows(in tableView: ImportCandidateTableView) {
        var stale = IndexSet()
        let made = tableView.rows(in: tableView.preparedContentRect)
        guard made.length > 0 else { return }
        for row in made.location..<(made.location + made.length)
        where row < rowCount {
            let current = item(at: row)
            let cell =
                tableView.view(atColumn: 0, row: row, makeIfNecessary: false)
                as? ImportListEntryCell
            if cell?.shownItem != current {
                stale.insert(row)
            }
        }
        guard !stale.isEmpty else { return }
        tableView.reloadData(
            forRowIndexes: stale,
            columnIndexes: IndexSet(integer: 0)
        )
    }

    /// Draw every row anew, and the pinned header with them.
    private func reloadRows() {
        tableView?.reloadData()
        pinnedStrip.forget()
    }

    /// The rows of the list as it stands, whatever the last update loaded:
    /// a reveal's position may come before the update that loads its rows.
    private func loadCurrentRows() {
        load(
            ImportCandidateTableRows(
                count: list.positionCount,
                revision: list.contentRevision
            )
        )
    }

    /// Keep the person's place while the rows move: core says where the
    /// candidate that was on top sits now, and the table puts it back on
    /// top. Nothing scrolls while a reveal does the scrolling, while the
    /// table goes back to its place, or when the rows did not change. A
    /// place at a position alone stays at that position.
    private func keepPlace(revealing: Bool, rowsChanged: Bool) {
        guard !revealing, rowsChanged, pendingPlace == nil,
            let place = listSlot.place, case .candidate = place.row
        else { return }
        keepTask?.cancel()
        let listSlot = listSlot
        keepTask = Task { [weak self] in
            let located = await listSlot.position(ofPlace: place)
            guard let self, !Task.isCancelled else { return }
            keepTask = nil
            if let located {
                loadCurrentRows()
                scroll(toTop: located, offset: place.offset)
            }
            recordPlace()
        }
    }

    /// Put `row` on top: a view change opened the list there, and the slot
    /// already holds it as the person's place, so the table does not record
    /// its current scroll position as the place. When the slot rereads the
    /// list for the opening, the sidebar replaces the table until the count
    /// query returns, and the new table goes to that place when it mounts.
    private func showOpening(_ row: Int) {
        placeTask?.cancel()
        placeTask = nil
        keepTask?.cancel()
        keepTask = nil
        pendingPlace = nil
        anchorKey = nil
        loadCurrentRows()
        scroll(toTop: row, offset: 0)
    }
}

// MARK: - Rows

extension ImportCandidateTableCoordinator {
    func numberOfRows(in _: NSTableView) -> Int {
        rowCount
    }

    /// How many rows the table was last loaded with.
    var rowCount: Int {
        shownRows?.count ?? 0
    }

    /// The entry at `row`, once its page has landed.
    func item(at row: Int) -> BridgeImportListItem? {
        list.row(at: row)
    }

    /// A header is its own, shorter height; every other row is one height,
    /// where the table would give a group row the shorter height itself.
    func tableView(_: NSTableView, heightOfRow row: Int) -> CGFloat {
        height(ofRow: row)
    }

    /// The height the row at `row` needs: a header's, once its page has
    /// landed, else a release's.
    private func height(ofRow row: Int) -> CGFloat {
        if case .groupHeader = item(at: row) {
            return ImportCandidateTableLayout.headerRowHeight
        }
        return ImportCandidateTableLayout.rowHeight
    }

    /// A header's row is drawn as a group row. The table does not float it:
    /// it knows the header rows only of the pages that have landed, and
    /// nothing of where a group ends, so the coordinator pins the header of
    /// the group on top itself.
    func tableView(_: NSTableView, isGroupRow row: Int) -> Bool {
        if case .groupHeader = item(at: row) { return true }
        return false
    }

    func tableView(_: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        guard case .groupHeader = item(at: row) else {
            return BrowsedTableRowView(accent: environment.appearance.accent)
        }
        return ImportListGroupRowView()
    }

    func tableView(
        _ tableView: NSTableView,
        viewFor _: NSTableColumn?,
        row: Int
    ) -> NSView? {
        guard let item = item(at: row) else {
            // A row drawn before its page lands asks for the page, and the
            // page's delivery reloads the table.
            askForPage(ofDrawnRow: row, in: tableView)
            return reused(tableView, Self.placeholder) { _ in
                NSTableCellView()
            }
        }
        let cell = reused(tableView, Self.identifier(for: item)) {
            ImportListEntryCell(identifier: $0)
        }
        cellsShown += 1
        cell.show(
            ImportListEntry(
                item: item,
                coverContent: item.triageRow.flatMap {
                    importStore.sidebarCover(for: $0)
                },
                actions: actions,
                environment: environment
            ),
            displayScale: tableView.window?.backingScaleFactor
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
        if let tableView {
            // The list sizes its window from the rows the table fits.
            list.fit(
                height: tableView.visibleRect.height,
                rowHeight: ImportCandidateTableLayout.rowHeight
            )
        }
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
        releaseDrawnHolds()
    }

    /// A row drawn for display, which the table draws before the rows in
    /// view take their holds, asks for its page while the table draws it: a
    /// row a fast scroll carries past stops being drawn a frame later and
    /// lets its page go. A row drawn off screen is one accessibility reads,
    /// and it asks for nothing: the list holds one window, which follows the
    /// rows on screen, so a page asked for off screen would move the window
    /// away from them. Such a row draws as a placeholder until the person or
    /// accessibility scrolls it into view.
    private func askForPage(ofDrawnRow row: Int, in tableView: NSTableView) {
        guard
            tableView.preparedContentRect.intersects(
                tableView.rect(ofRow: row)
            ),
            drawnHolds[row] == nil
        else { return }
        let list = list
        drawnHolds[row] = Task { await list.holdPage(containing: row) }
    }

    /// A row the table stops drawing lets its page go.
    func tableView(
        _: NSTableView,
        didRemove _: NSTableRowView,
        forRow row: Int
    ) {
        drawnHolds.removeValue(forKey: row)?.cancel()
    }

    /// Every drawn row lets its page go: the list they were drawn from is
    /// gone.
    private func releaseDrawnHolds() {
        for hold in drawnHolds.values {
            hold.cancel()
        }
        drawnHolds.removeAll()
    }
}

// MARK: - Scrolling and place

extension ImportCandidateTableCoordinator {
    /// A scroll during an update comes from the rows moving under the table.
    /// Any other scroll is the person's, or one the table makes after it
    /// stops keeping the place, so the table stops keeping the place and
    /// records where it is now.
    private func clipViewScrolled() {
        holdRowsInView()
        refreshPinnedHeader()
        if !updating {
            keepTask?.cancel()
            keepTask = nil
        }
        recordPlace()
    }

    private func clipViewResized() {
        holdRowsInView()
        refreshPinnedHeader()
        if let pendingPlace, placeTask == nil,
            let clipView = scrollView?.contentView, clipView.bounds.height > 0
        {
            goBack(to: pendingPlace)
        }
        else {
            recordPlace()
        }
    }

    /// Go back to `place` in a table mounted anew: to where the row was,
    /// then, for a candidate's row, to where core says it is now.
    private func goBack(to place: ImportListPlace) {
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

    /// Stop going back to the pending place and stop keeping the place, since
    /// the table is there or the person or a reveal moved it, and take where
    /// it is as the place.
    func endPendingPlace() {
        placeTask?.cancel()
        placeTask = nil
        keepTask?.cancel()
        keepTask = nil
        guard pendingPlace != nil else { return }
        pendingPlace = nil
        recordPlace()
    }

    /// Take the row on top of the table, and how far into it the table is
    /// scrolled, as the person's place.
    private func recordPlace() {
        guard !updating, pendingPlace == nil, keepTask == nil, let tableView,
            let clipView = scrollView?.contentView, let row = topRow
        else { return }
        let top = clipView.bounds.minY
        listSlot.place = ImportListPlace(
            row: placeRow(at: row),
            position: row,
            offset: top - tableView.rect(ofRow: row).minY
        )
    }

    private func placeRow(at row: Int) -> ImportListPlace.Row {
        guard let candidate = list.row(at: row)?.triageRow else {
            return .atPosition
        }
        return .candidate(candidateKey: candidate.candidateKey)
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

// MARK: - Pinned header

extension ImportCandidateTableCoordinator {
    /// The row under the table's top edge, not the header pinned over it.
    fileprivate var topRow: Int? {
        guard let tableView, let clipView = scrollView?.contentView,
            rowCount > 0
        else {
            return nil
        }
        let row = tableView.row(
            at: NSPoint(x: 0, y: max(clipView.bounds.minY, 0))
        )
        return row < 0 ? rowCount - 1 : min(row, rowCount - 1)
    }

    /// The header of the group the row on top is listed under, as that row
    /// carries it.
    func pinnedHeader() -> ImportListPinnedHeader? {
        guard let top = topRow else { return nil }
        return ImportListPinnedHeader.over(top: list.row(at: top))
    }

    /// Pin the header of the group on top over the rows, once its page has
    /// landed: lifted with the row on top when that row ends its group, and
    /// none over a release in no group.
    fileprivate func refreshPinnedHeader() {
        guard let tableView, let scrollView, let top = topRow,
            let pinned = pinnedHeader()
        else {
            pinnedStrip.hide()
            return
        }
        let lift =
            pinned.ends
            ? scrollView.contentView.bounds.minY
                - tableView.rect(ofRow: top).minY
            : 0
        pinnedStrip.show(
            ImportListEntry(
                item: .groupHeader(header: pinned.header),
                coverContent: nil,
                actions: actions,
                environment: environment
            ),
            lift: lift,
            in: scrollView
        )
    }
}

extension BridgeImportListItem {
    /// The candidate's row, for a candidate's entry.
    var triageRow: BridgeTriageRow? {
        if case .candidate(_, let row, _) = self { return row }
        return nil
    }
}
