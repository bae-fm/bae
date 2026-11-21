import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("StorageTable")

// MARK: - Representable

/// An `NSTableView`-backed storage table: AppKit handles the table
/// mechanics (resizable / reorderable columns, header sort indicators) with
/// SwiftUI-hosted cell content so covers and the storage badge reuse
/// `Artwork` / `Theme`. The columns' order, widths and which are hidden are
/// `layout`, which the table applies and writes back.
///
/// Releases load a page at a time, each page held while a row of it is in
/// view. Their files belong to the selected-release inspector, not to child
/// rows in this table. The selection is the release ids the view keeps, which
/// each row in view draws its highlight from; column sorting maps to
/// `BridgeStorageSort`.
struct MacTable: NSViewRepresentable {
    let list: StorageList
    @Binding
    var selection: Set<String>
    @Binding
    var sort: BridgeStorageSort
    @Binding
    var inspectorPresented: Bool
    @Binding
    var layout: StorageTableLayout
    let runner: StorageActionRunner

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(StorageManagerStore.self)
    private var storageManagerStore
    @Environment(\.surfaceTone)
    private var tone
    @Environment(\.accentChoice)
    private var accent

    func makeCoordinator() -> Coordinator {
        Coordinator(
            list: list,
            selection: $selection,
            sort: $sort,
            inspectorPresented: $inspectorPresented,
            layout: $layout,
            storageManagerStore: storageManagerStore,
            runner: runner,
            imageStore: imageStore,
            downloadStore: downloadStore,
            outboxStore: outboxStore,
            appearance: appearance,
        )
    }

    private var appearance: HostedAppearance {
        HostedAppearance(tone: tone, accent: accent)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let coordinator = context.coordinator
        let tableView = coordinator.table
        tableView.menus = coordinator
        tableView.onSelectAll = { [weak coordinator] in
            coordinator?.selectAllShown()
        }
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        tableView.target = coordinator
        tableView.doubleAction = #selector(
            Coordinator.handleInspectorDoubleClick(_:)
        )
        tableView.allowsMultipleSelection = true
        tableView.allowsColumnReordering = true
        tableView.allowsColumnResizing = true
        tableView.columnAutoresizingStyle =
            .firstColumnOnlyAutoresizingStyle
        tableView.rowHeight = 28
        tableView.style = .inset

        for column in StorageTableColumn.allCases {
            let tableColumn = NSTableColumn(
                identifier: NSUserInterfaceItemIdentifier(column.rawValue)
            )
            tableColumn.title = column.title
            tableColumn.width = column.width
            tableColumn.minWidth = column.minWidth
            tableColumn.isEditable = false
            tableColumn.sortDescriptorPrototype = NSSortDescriptor(
                key: column.rawValue,
                ascending: true
            )
            tableView.addTableColumn(tableColumn)
        }
        let header = StorageTableHeader()
        header.menus = coordinator
        tableView.headerView = header
        coordinator.applySortIndicator(to: tableView, sort: sort)

        let scrollView = NSScrollView()
        scrollView.documentView = tableView
        // Applied in the scroll view, whose width the first column fits.
        coordinator.applyLayout(layout)
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        coordinator.observeScrolling(of: scrollView)
        return scrollView
    }

    func updateNSView(_: NSScrollView, context: Context) {
        let coordinator = context.coordinator
        let tableView = coordinator.table
        // Reading these `@Observable` fields here is what makes SwiftUI
        // re-invoke `updateNSView` when the list grows, changes, or is swapped
        // for a fresh instance on a sort/filter change.
        let totalCount = list.positionCount
        let epoch = list.loadEpoch
        // The rows held the swapped-out list's pages.
        if coordinator.list !== list {
            coordinator.releasePages()
        }

        coordinator.update(
            list: list,
            imageStore: imageStore,
            downloadStore: downloadStore,
            outboxStore: outboxStore,
        )
        let appearanceChanged = coordinator.updateAppearance(appearance)

        // A new generation or changed count means the rows changed identity or
        // length, so the table must reload from the list; each cell follows
        // the row at its position on its own. A changed tone or accent redraws
        // the cells, which host their content apart from the window and so
        // don't follow it on their own.
        let rowsChanged = coordinator.syncRows(
            totalCount: totalCount,
            epoch: epoch
        )
        if rowsChanged || appearanceChanged {
            tableView.reloadData()
        }
        coordinator.holdRowsInView()

        coordinator.applyLayout(layout)
        coordinator.applySortIndicator(to: tableView, sort: sort)
        coordinator.applySelection(selection)
    }

    static func dismantleNSView(
        _ scrollView: NSScrollView,
        coordinator: Coordinator
    ) {
        coordinator.releasePages()
        coordinator.cancelSelectionRead()
    }
}

// MARK: - Coordinator

extension MacTable {
    @MainActor
    final class Coordinator: NSObject, NSTableViewDataSource,
        NSTableViewDelegate
    {
        private(set) var list: StorageList
        /// The release ids the view keeps selected, which each row in view
        /// draws its highlight from.
        let selection: Binding<Set<String>>
        /// The release a Shift gesture extends from: the one the last gesture
        /// without Shift landed on.
        var anchorId: String?
        private let sort: Binding<BridgeStorageSort>
        private let inspectorPresented: Binding<Bool>
        /// The column layout the table shows, which column changes write.
        let layout: Binding<StorageTableLayout>
        /// The layout the table last took from `layout` or wrote to it,
        /// which its columns hold but for the first column's width: that
        /// follows the window.
        var appliedLayout: StorageTableLayout?
        /// True while `applyLayout` moves and resizes the table's columns,
        /// whose notifications then carry no change of the person's.
        var applyingLayout = false
        /// The store that narrows the selection to the releases the list
        /// holds and answers with each one's row, so nothing here acts on or
        /// counts a selected release the list no longer shows. The
        /// coordinator also asks the store for the releases a Shift gesture
        /// selects and for every release Select All selects.
        let storageManagerStore: StorageManagerStore
        private let runner: StorageActionRunner
        private var imageStore: ImageStore
        private var downloadStore: DownloadStore
        private var outboxStore: OutboxStore
        /// The tone and accent of the window the table is in, which its
        /// hosted cells draw in.
        private(set) var appearance: HostedAppearance
        /// The table the coordinator serves, for as long as the coordinator
        /// lives.
        let table: StorageTable

        private var rootEpoch: LoadEpoch?
        private(set) var rootCount = 0
        /// Each row in view's hold on its page, by row.
        private var rowHolds: [Int: Task<Void, Never>] = [:]

        /// True while the coordinator sets the table's highlight, which is
        /// no person's change to the selection.
        var applyingSelection = false
        /// The read of core's ids that the last Shift gesture or Select All
        /// started. A later gesture or a new list cancels it.
        var selectionRead: Task<Void, Never>?

        init(
            list: StorageList,
            selection: Binding<Set<String>>,
            sort: Binding<BridgeStorageSort>,
            inspectorPresented: Binding<Bool>,
            layout: Binding<StorageTableLayout>,
            storageManagerStore: StorageManagerStore,
            runner: StorageActionRunner,
            imageStore: ImageStore,
            downloadStore: DownloadStore,
            outboxStore: OutboxStore,
            appearance: HostedAppearance,
        ) {
            self.list = list
            self.selection = selection
            self.sort = sort
            self.inspectorPresented = inspectorPresented
            self.layout = layout
            self.storageManagerStore = storageManagerStore
            self.runner = runner
            self.imageStore = imageStore
            self.downloadStore = downloadStore
            self.outboxStore = outboxStore
            self.appearance = appearance
            table = StorageTable()
        }

        func update(
            list: StorageList,
            imageStore: ImageStore,
            downloadStore: DownloadStore,
            outboxStore: OutboxStore,
        ) {
            // A new list cancels the read, whose ids are the old list's
            // releases.
            if list !== self.list {
                cancelSelectionRead()
            }
            self.list = list
            self.imageStore = imageStore
            self.downloadStore = downloadStore
            self.outboxStore = outboxStore
        }

        /// Take the window's tone and accent. Returns whether either changed,
        /// which the drawn cells must be redrawn for.
        func updateAppearance(_ appearance: HostedAppearance) -> Bool {
            let changed = appearance != self.appearance
            self.appearance = appearance
            return changed
        }

        /// Detect a changed list generation or row count. Returns whether the
        /// table must reload.
        func syncRows(totalCount: Int, epoch: LoadEpoch) -> Bool {
            guard rootEpoch != epoch || rootCount != totalCount else {
                return false
            }
            rootEpoch = epoch
            rootCount = totalCount
            return true
        }

        func numberOfRows(in tableView: NSTableView) -> Int {
            rootCount
        }

        // MARK: Page holds

        /// Hold the pages of the rows in view as the table scrolls or is
        /// resized.
        func observeScrolling(of scrollView: NSScrollView) {
            let clipView = scrollView.contentView
            clipView.postsBoundsChangedNotifications = true
            clipView.postsFrameChangedNotifications = true
            for name in [
                NSView.boundsDidChangeNotification,
                NSView.frameDidChangeNotification,
            ] {
                NotificationCenter.default.addObserver(
                    self,
                    selector: #selector(clipViewMoved),
                    name: name,
                    object: clipView
                )
            }
        }

        /// A scroll brings other rows into view, each highlighted when the
        /// kept ids name its release.
        @objc
        private func clipViewMoved() {
            holdRowsInView()
            applySelection(selection.wrappedValue)
        }

        /// Each row in view holds its page; a row leaving the view lets its
        /// hold go.
        func holdRowsInView() {
            // The list sizes its window from the rows the table fits.
            list.fit(
                height: table.visibleRect.height,
                rowHeight: table.rowHeight
            )
            let range = table.rows(in: table.visibleRect)
            let inView = Set(range.location..<(range.location + range.length))
            for (row, hold) in rowHolds where !inView.contains(row) {
                hold.cancel()
                rowHolds.removeValue(forKey: row)
            }
            for row in inView where rowHolds[row] == nil {
                rowHolds[row] = holdPage(of: row)
            }
        }

        /// Hold `row`'s page until the task is cancelled. A row whose page had
        /// not landed draws its placeholder until the page answers, when its
        /// cells draw the row and, if it is selected, the table highlights it.
        private func holdPage(of row: Int) -> Task<Void, Never> {
            let list = list
            let landed = list.idAt(row) != nil
            return Task { @MainActor [weak self] in
                await list.holdPage(containing: row) {
                    // Only the still-mounted list's selection applies; a tab
                    // switch swaps the list before this resolves.
                    if !landed, let self, list === self.list {
                        self.applySelection(self.selection.wrappedValue)
                    }
                }
            }
        }

        /// Let go of every page the table holds: its list was swapped, or it
        /// is going away.
        func releasePages() {
            for hold in rowHolds.values {
                hold.cancel()
            }
            rowHolds.removeAll()
        }
    }
}

// MARK: - Sorting & selection

extension MacTable.Coordinator {
    func tableView(
        _ tableView: NSTableView,
        sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]
    ) {
        guard let descriptor = tableView.sortDescriptors.first,
            let key = descriptor.key,
            let field = sortField(forDescriptorKey: key)
        else {
            return
        }
        // Server-side sort: write the new `BridgeStorageSort` back through
        // the binding so the view's `rebuildList()` runs. The header
        // indicator already reflects the click.
        sort.wrappedValue = BridgeStorageSort(
            field: field,
            direction: descriptor.ascending ? .ascending : .descending
        )
    }

    /// Reflect the active `BridgeStorageSort` in the header indicator
    /// without re-triggering `sortDescriptorsDidChange` (set the array
    /// directly rather than mutating the column).
    func applySortIndicator(
        to tableView: NSTableView,
        sort: BridgeStorageSort
    ) {
        guard
            let column = StorageTableColumn.allCases.first(where: {
                $0.sortField == sort.field
            })
        else {
            logger.error(
                "Sort field \(String(describing: sort.field)) maps to no column"
            )
            return
        }
        let descriptor = NSSortDescriptor(
            key: column.rawValue,
            ascending: sort.direction == .ascending
        )
        if tableView.sortDescriptors.first != descriptor {
            tableView.sortDescriptors = [descriptor]
        }
    }

    @objc
    func handleInspectorDoubleClick(_ sender: NSTableView) {
        let row =
            sender.clickedRow >= 0 ? sender.clickedRow : sender.selectedRow
        guard let releaseId = list.idAt(row) else {
            return
        }
        let isOpenForRelease =
            inspectorPresented.wrappedValue
            && storageManagerStore.soleRelease == releaseId
        setInspectorPresented(!isOpenForRelease, releaseId: releaseId)
    }
}

// MARK: - Cell views

extension MacTable.Coordinator {
    func tableView(
        _ tableView: NSTableView,
        viewFor tableColumn: NSTableColumn?,
        row: Int
    ) -> NSView? {
        guard
            let tableColumn,
            let column = StorageTableColumn(
                rawValue: tableColumn.identifier.rawValue
            )
        else {
            logger.error(
                "No storage column for \(tableColumn?.identifier.rawValue ?? "nil")"
            )
            return nil
        }
        let cell = dequeueCell(tableView, column: column)
        cell.host(content(forRow: row, column: column))
        return cell
    }

    func tableView(_: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        BrowsedTableRowView(
            accent: appearance.accent,
            alternate: row % 2 == 1
        )
    }

    private func dequeueCell(
        _ tableView: NSTableView,
        column: StorageTableColumn
    ) -> HostingTableCell {
        let identifier = NSUserInterfaceItemIdentifier(column.rawValue)
        if let reused = tableView.makeView(
            withIdentifier: identifier,
            owner: self
        ) as? HostingTableCell {
            return reused
        }
        let cell = HostingTableCell()
        cell.identifier = identifier
        return cell
    }

    /// SwiftUI content for a cell, wrapped with the environment values the
    /// hosted cell views read at the leaf (`Artwork` needs `ImageStore`;
    /// the storage chip reads `DownloadStore` and `OutboxStore`), and the
    /// window's tone and accent, which a hosted cell doesn't inherit.
    @ViewBuilder
    func content(
        forRow row: Int,
        column: StorageTableColumn
    ) -> some View {
        StorageListCell(list: list, position: row, column: column)
            .environment(imageStore)
            .environment(downloadStore)
            .environment(outboxStore)
            .hostedAppearance(appearance)
    }

    // MARK: Context menu

    /// Opens bae's menu for a right click on `row` of `tableView`, at the
    /// pointer. A click below the rows opens none.
    func showContextMenu(forRow row: Int, in tableView: NSTableView) {
        guard let window = tableView.window else { return }
        MacMenus.present(
            contextEntries(forRow: row),
            over: window,
            at: .atPointer,
            appearance: appearance
        )
    }

    /// The lines of the menu for a right click on `row`.
    func contextEntries(forRow clickedRow: Int) -> [MacMenuEntry] {
        guard clickedRow >= 0, let clicked = list.row(at: clickedRow) else {
            return []
        }
        let clickedId = clicked.id
        var entries: [MacMenuEntry] = [
            .item(
                MacMenuItem("Inspect") { [self] in
                    setInspectorPresented(true, releaseId: clickedId)
                }
            ),
            .divider,
        ]

        // Right-click acts on the selection when the clicked row is part of it,
        // otherwise on just that row (and selects it).
        let targets = menuTargets(forClicked: clicked)
        guard !targets.rows.isEmpty else { return entries }
        let releaseIds = targets.rows.map(\.id)

        // A release mid-transition (uploading, pinning, or becoming local) can't
        // start another storage action (those race the transition), so the only
        // action it offers is cancelling — in every tab. So does a release
        // whose move failed: cancelling ends the move. Core says whether any
        // target is one.
        if targets.cancelOnly {
            entries.append(
                .item(
                    MacMenuItem("Cancel") { [runner] in
                        runner.cancelTransitions(releaseIds: releaseIds)
                    }
                )
            )
            return entries
        }
        for action in targets.actions {
            entries.append(
                .item(
                    MacMenuItem(verbatim: action.label) { [runner] in
                        runner.run(
                            action,
                            releaseIds: releaseIds,
                            soleTitle: targets.soleTitle
                        )
                    }
                )
            )
        }
        // Export (verbatim) and Save As (preset workup) are pure outputs that
        // change no state, so both are offered for every release regardless of
        // locality — not among the core-computed `storageActions`.
        entries.append(
            .item(
                MacMenuItem("Export…") { [runner] in
                    runner.export(releaseIds: releaseIds)
                }
            )
        )
        entries.append(
            .item(
                MacMenuItem("Save as…") { [runner] in
                    runner.saveAs(releaseIds: releaseIds)
                }
            )
        )
        return entries
    }

    /// If the clicked release is in the selection, act on the rows core
    /// answered for it, with the storage actions core says they all allow;
    /// otherwise act on and select that release, with its own. Each
    /// target is the row the list's query returned for it: the clicked one
    /// is the row the table draws at the clicked position.
    private func menuTargets(
        forClicked clicked: BridgeStorageRow
    ) -> MenuTargets {
        if let answer = storageManagerStore.answered(
            for: selection.wrappedValue
        ),
            selection.wrappedValue.contains(clicked.id)
        {
            return MenuTargets(
                rows: answer.selected,
                actions: answer.selectionActions,
                cancelOnly: answer.cancelOnly,
                soleTitle: answer.sole?.albumTitle
            )
        }
        select([clicked.id])
        anchorId = clicked.id
        applySelection([clicked.id])
        return MenuTargets(
            rows: [clicked],
            actions: clicked.release.storageActions,
            cancelOnly: clicked.cancelOnly,
            soleTitle: clicked.album.title
        )
    }

    /// The releases a menu acts on, the storage actions they all allow,
    /// whether Cancel is the only action one of them offers, and the album
    /// title of the one release it acts on, when it acts on one.
    private struct MenuTargets {
        let rows: [BridgeStorageRow]
        let actions: [BridgeReleaseStorageAction]
        let cancelOnly: Bool
        let soleTitle: String?
    }

    private func setInspectorPresented(
        _ presented: Bool,
        releaseId: String
    ) {
        select([releaseId])
        anchorId = releaseId
        applySelection([releaseId])
        inspectorPresented.wrappedValue = presented
    }
}

/// The storage table, whose right click opens bae's menu in place of
/// AppKit's. The table keeps the mouse or key press it is handling, so the
/// coordinator can read the modifier keys and the pressed row behind each
/// selection change the table proposes. Select All asks the coordinator to
/// select every release the list shows, not only the rows the table has
/// loaded.
final class StorageTable: NSTableView {
    weak var menus: MacTable.Coordinator?
    /// The mouse or key press the table is handling, while it handles it.
    private(set) var handlingEvent: NSEvent?
    /// Select every release the list shows, loaded or not.
    var onSelectAll: (() -> Void)?

    override func mouseDown(with event: NSEvent) {
        handling(event) { super.mouseDown(with: event) }
    }

    override func keyDown(with event: NSEvent) {
        handling(event) { super.keyDown(with: event) }
    }

    override func selectAll(_ sender: Any?) {
        onSelectAll?()
    }

    /// The row `event` pressed, for a click on a row.
    func pressedRow(_ event: NSEvent) -> Int? {
        guard event.type == .leftMouseDown else { return nil }
        let row = row(at: convert(event.locationInWindow, from: nil))
        return row >= 0 ? row : nil
    }

    private func handling(_ event: NSEvent, _ handle: () -> Void) {
        let outer = handlingEvent
        handlingEvent = event
        defer { handlingEvent = outer }
        handle()
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let row = row(at: convert(event.locationInWindow, from: nil))
        menus?.showContextMenu(forRow: row, in: self)
        return nil
    }

    /// The header's column buttons first, then the rows: a person reading
    /// the table with VoiceOver meets the column names before the cells.
    override func accessibilityChildren() -> [Any]? {
        guard let children = super.accessibilityChildren() else { return nil }
        guard let header = headerView else { return children }
        let isHeader: (Any) -> Bool = { child in
            guard let view = child as? NSView else { return false }
            return view === header || view.isDescendant(of: header)
        }
        return children.filter(isHeader) + children.filter { !isHeader($0) }
    }
}

/// The storage table's header, whose right click opens bae's columns menu.
final class StorageTableHeader: NSTableHeaderView {
    weak var menus: MacTable.Coordinator?

    override func menu(for event: NSEvent) -> NSMenu? {
        menus?.showColumnsMenu(in: self)
        return nil
    }
}
