import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("StorageTable")

// MARK: - Representable

/// An `NSTableView`-backed storage table: AppKit handles the table
/// mechanics (resizable / reorderable columns, header sort indicators,
/// shift/cmd selection) with SwiftUI-hosted cell content so covers and the
/// storage badge reuse `ImageView` / `Theme`.
///
/// Releases load a page at a time, each page held while a row of it is in
/// view. Their files belong to the selected-release inspector, not to child
/// rows in this table. Selection binds to release ids; column sorting maps to
/// `BridgeStorageSort`.
struct StorageTableView: NSViewRepresentable {
    let list: StorageList
    @Binding
    var selection: Set<String>
    @Binding
    var sort: BridgeStorageSort
    @Binding
    var inspectorPresented: Bool
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
        let tableView = NSTableView()
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        tableView.menu = coordinator.makeContextMenu()
        tableView.target = coordinator
        tableView.doubleAction = #selector(
            Coordinator.handleInspectorDoubleClick(_:)
        )
        tableView.allowsMultipleSelection = true
        tableView.allowsColumnReordering = true
        tableView.allowsColumnResizing = true
        tableView.usesAlternatingRowBackgroundColors = true
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
        coordinator.applySortIndicator(to: tableView, sort: sort)

        let scrollView = NSScrollView()
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        coordinator.tableView = tableView
        coordinator.observeScrolling(of: scrollView)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let tableView = scrollView.documentView as? NSTableView else {
            logger.error(
                "Storage scroll view is missing its table document view"
            )
            return
        }
        let coordinator = context.coordinator
        // Reading these `@Observable` fields here is what makes SwiftUI
        // re-invoke `updateNSView` when the list grows, changes, or is swapped
        // for a fresh instance on a sort/filter change.
        let totalCount = list.totalCount
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

        coordinator.applySortIndicator(to: tableView, sort: sort)
        coordinator.applySelection(selection, to: tableView)
    }

    static func dismantleNSView(
        _ scrollView: NSScrollView,
        coordinator: Coordinator
    ) {
        coordinator.releasePages()
    }
}

// MARK: - Coordinator

extension StorageTableView {
    @MainActor
    final class Coordinator: NSObject, NSTableViewDataSource,
        NSTableViewDelegate
    {
        private(set) var list: StorageList
        private let selection: Binding<Set<String>>
        private let sort: Binding<BridgeStorageSort>
        private let inspectorPresented: Binding<Bool>
        /// Narrows the selection to the releases the list holds, and answers
        /// with each one's row: nothing here acts on or counts a selected
        /// release the list no longer shows.
        private let storageManagerStore: StorageManagerStore
        private let runner: StorageActionRunner
        private var imageStore: ImageStore
        private var downloadStore: DownloadStore
        private var outboxStore: OutboxStore
        /// The tone and accent of the window the table is in, which its
        /// hosted cells draw in.
        private var appearance: HostedAppearance
        weak var tableView: NSTableView?

        private var rootEpoch: LoadEpoch?
        private var rootCount = 0
        /// Each row in view's hold on its page, by row.
        private var rowHolds: [Int: Task<Void, Never>] = [:]

        /// True while we push selection into the table view, so the
        /// resulting `selectionDidChange` delegate callback doesn't write the
        /// same value back through the binding (and fight the next update).
        private var applyingSelection = false

        init(
            list: StorageList,
            selection: Binding<Set<String>>,
            sort: Binding<BridgeStorageSort>,
            inspectorPresented: Binding<Bool>,
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
            self.storageManagerStore = storageManagerStore
            self.runner = runner
            self.imageStore = imageStore
            self.downloadStore = downloadStore
            self.outboxStore = outboxStore
            self.appearance = appearance
        }

        func update(
            list: StorageList,
            imageStore: ImageStore,
            downloadStore: DownloadStore,
            outboxStore: OutboxStore,
        ) {
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

        @objc
        private func clipViewMoved() {
            holdRowsInView()
        }

        /// Each row in view holds its page; a row leaving the view lets its
        /// hold go.
        func holdRowsInView() {
            guard let tableView else { return }
            let range = tableView.rows(in: tableView.visibleRect)
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
                await list.withPage(containing: row) {
                    // Only the still-mounted list's selection applies; a tab
                    // switch swaps the list before this resolves.
                    if !landed, let self, list === self.list {
                        self.applySelection(
                            self.selection.wrappedValue,
                            to: self.tableView
                        )
                    }
                    await Task.untilCancelled()
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

extension StorageTableView.Coordinator {
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

    // MARK: Selection

    func tableViewSelectionDidChange(_ notification: Notification) {
        guard !applyingSelection,
            let tableView = notification.object as? NSTableView
        else {
            return
        }
        var ids: Set<String> = []
        for row in tableView.selectedRowIndexes {
            if let id = list.idAt(row) {
                ids.insert(id)
            }
        }
        if ids != selection.wrappedValue {
            selection.wrappedValue = ids
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
            && storageManagerStore.shown(selection.wrappedValue) == [releaseId]
        setInspectorPresented(!isOpenForRelease, releaseId: releaseId)
    }

    /// Push the bound release-id selection into the table view's row
    /// selection, mapping each id to its loaded position. Guarded so the
    /// resulting delegate callback doesn't echo back through the binding.
    func applySelection(
        _ ids: Set<String>,
        to tableView: NSTableView?
    ) {
        guard let tableView else { return }
        var rows = IndexSet()
        for id in ids {
            // `position(of:)` reads loaded segments, which can briefly hold a
            // position past a freshly-rebuilt row count while a live page is
            // being installed. A selected id whose row isn't loaded can't be
            // highlighted yet; skip it.
            guard let position = list.position(of: id),
                position < rootCount
            else {
                logger.debug(
                    "Selected release \(id) has no loaded row to highlight"
                )
                continue
            }
            rows.insert(position)
        }
        guard rows != tableView.selectedRowIndexes else {
            return
        }
        applyingSelection = true
        tableView.selectRowIndexes(rows, byExtendingSelection: false)
        applyingSelection = false
    }
}

// MARK: - Cell views

extension StorageTableView.Coordinator {
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
    /// hosted cell views read at the leaf (`ImageView` needs `ImageStore`;
    /// the storage chip reads `DownloadStore` and `OutboxStore`), and the
    /// window's tone and accent, which a hosted cell doesn't inherit.
    @ViewBuilder
    private func content(
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

    func makeContextMenu() -> NSMenu {
        let menu = NSMenu()
        menu.delegate = self
        return menu
    }
}

// MARK: - Context menu population

extension StorageTableView.Coordinator: NSMenuDelegate {
    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        guard let tableView else { return }
        let clickedRow = tableView.clickedRow
        guard clickedRow >= 0, let clickedId = list.idAt(clickedRow) else {
            return
        }

        addInspectMenuItem(to: menu, releaseId: clickedId)

        // Right-click acts on the selection when the clicked row is part of it,
        // otherwise on just that row (and selects it).
        let (targets, actions) = menuTargets(forClickedRelease: clickedId)
        guard !targets.isEmpty else { return }

        // A release mid-transition (uploading, pinning, or becoming local) can't
        // start another storage action (those race the transition), so the only
        // action it offers is cancelling — in every tab.
        let transitioning = targets.filter(\.release.transitioning)
        if !transitioning.isEmpty {
            addMenuItem(
                to: menu,
                title: String(localized: "Cancel"),
                action: #selector(cancelTransitionsAction(_:)),
                symbol: "xmark.circle",
                representedObject: transitioning.map(\.id)
            )
            return
        }
        for action in actions {
            addMenuItem(
                to: menu,
                title: action.label,
                action: #selector(runStorageAction(_:)),
                symbol: action.systemImage,
                representedObject: StorageActionInvocation(
                    action: action,
                    releaseIds: targets.map(\.id)
                )
            )
        }
        // Export (verbatim) and Save As (preset workup) are pure outputs that
        // change no state, so both are offered for every release regardless of
        // locality — not among the core-computed `storageActions`.
        addMenuItem(
            to: menu,
            title: String(localized: "Export…"),
            action: #selector(runExportAction(_:)),
            symbol: "square.and.arrow.up",
            representedObject: targets.map(\.id)
        )
        addMenuItem(
            to: menu,
            title: String(localized: "Save As…"),
            action: #selector(runSaveAsAction(_:)),
            symbol: "square.and.arrow.down",
            representedObject: targets.map(\.id)
        )
    }

    private func addInspectMenuItem(to menu: NSMenu, releaseId: String) {
        addMenuItem(
            to: menu,
            title: String(localized: "Inspect"),
            action: #selector(inspectReleaseAction(_:)),
            symbol: "sidebar.trailing",
            representedObject: releaseId
        )
        menu.addItem(.separator())
    }

    /// Build a context-menu item targeting this coordinator and add it to the
    /// menu. Shared by the cancel-upload and storage-action branches.
    private func addMenuItem(
        to menu: NSMenu,
        title: String,
        action: Selector,
        symbol: String,
        representedObject: Any
    ) {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.image = NSImage(
            systemSymbolName: symbol,
            accessibilityDescription: nil
        )
        item.target = self
        item.representedObject = representedObject
        menu.addItem(item)
    }

    /// If the clicked release is already selected, act on the selected
    /// releases the list holds, with the storage actions core says they all
    /// allow; otherwise act on and select that release, with its own. Each
    /// target is the row the list's query returned for it.
    private func menuTargets(
        forClickedRelease clickedId: String
    ) -> (rows: [BridgeStorageRow], actions: [BridgeReleaseStorageAction]) {
        let shown = storageManagerStore.shown(selection.wrappedValue)
        guard shown.contains(clickedId) else {
            selection.wrappedValue = [clickedId]
            applySelection([clickedId], to: tableView)
            let row = targetRow(clickedId)
            return ([row], row.release.storageActions)
        }
        let actions = storageManagerStore.selectionActions(for: shown)
        if actions == nil {
            logger.debug(
                "No storage actions offered: core has not answered for the selection of \(shown.count) yet"
            )
        }
        return (shown.map(targetRow), actions ?? [])
    }

    /// The row of a release `shown` keeps, or of the clicked one, which is
    /// loaded.
    private func targetRow(_ id: String) -> BridgeStorageRow {
        guard let row = storageManagerStore.row(of: id) else {
            preconditionFailure("Release \(id) is shown but has no row")
        }
        return row
    }

    @objc
    private func inspectReleaseAction(_ sender: NSMenuItem) {
        guard let releaseId = sender.representedObject as? String else {
            logger.error("Inspect menu item carried no release id")
            return
        }
        setInspectorPresented(true, releaseId: releaseId)
    }

    private func setInspectorPresented(
        _ presented: Bool,
        releaseId: String
    ) {
        selection.wrappedValue = [releaseId]
        applySelection([releaseId], to: tableView)
        inspectorPresented.wrappedValue = presented
    }

    @objc
    private func runStorageAction(_ sender: NSMenuItem) {
        guard
            let invocation = sender.representedObject
                as? StorageActionInvocation
        else {
            logger.error("Storage action menu item carried no invocation")
            return
        }
        runner.run(invocation.action, releaseIds: invocation.releaseIds)
    }

    @objc
    private func cancelTransitionsAction(_ sender: NSMenuItem) {
        guard let releaseIds = sender.representedObject as? [String] else {
            logger.error("Cancel menu item carried no release ids")
            return
        }
        runner.cancelTransitions(releaseIds: releaseIds)
    }

    @objc
    private func runExportAction(_ sender: NSMenuItem) {
        guard let releaseIds = sender.representedObject as? [String] else {
            logger.error("Export menu item carried no release ids")
            return
        }
        runner.export(releaseIds: releaseIds)
    }

    @objc
    private func runSaveAsAction(_ sender: NSMenuItem) {
        guard let releaseIds = sender.representedObject as? [String] else {
            logger.error("Save As menu item carried no release ids")
            return
        }
        runner.saveAs(releaseIds: releaseIds)
    }

}

/// A pending menu action: which storage transition to run against which
/// releases. Boxed as a menu item's `representedObject`.
private final class StorageActionInvocation {
    let action: BridgeReleaseStorageAction
    let releaseIds: [String]
    init(action: BridgeReleaseStorageAction, releaseIds: [String]) {
        self.action = action
        self.releaseIds = releaseIds
    }
}

/// One cell of the storage table, drawn from the row the list's query
/// returned at `position`, or a placeholder until that row's page lands. It
/// reads the list itself, so it follows the row as the query answers again.
private struct StorageListCell: View {
    let list: StorageList
    let position: Int
    let column: StorageTableColumn

    var body: some View {
        if let row = list.row(at: position) {
            StorageReleaseCell(
                release: ReleaseSummary(from: row.release),
                album: row.album,
                status: row.status,
                column: column
            )
        }
        else {
            StorageRowPlaceholderCell(column: column)
        }
    }
}
