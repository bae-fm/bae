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
/// Releases are lazy-loaded in batches by position. Their files belong to the
/// selected-release inspector, not to child rows in this table. Selection binds
/// to release ids; column sorting maps to `BridgeStorageSort`.
struct StorageTableView: NSViewRepresentable {
    let list: StorageList
    @Binding
    var selection: Set<String>
    @Binding
    var sort: BridgeStorageSort
    @Binding
    var inspectorPresented: Bool
    let sortingEnabled: Bool
    let libraryStore: LibraryStore
    let runner: StorageActionRunner

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(OutboxStore.self)
    private var outboxStore

    func makeCoordinator() -> Coordinator {
        Coordinator(
            list: list,
            selection: $selection,
            sort: $sort,
            inspectorPresented: $inspectorPresented,
            sortingEnabled: sortingEnabled,
            libraryStore: libraryStore,
            runner: runner,
            imageStore: imageStore,
            outboxStore: outboxStore,
        )
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
            tableView.addTableColumn(tableColumn)
        }
        coordinator.configureSorting(on: tableView, sort: sort)

        let scrollView = NSScrollView()
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        coordinator.tableView = tableView
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

        coordinator.update(
            list: list,
            imageStore: imageStore,
            outboxStore: outboxStore,
            sortingEnabled: sortingEnabled,
        )

        // A new generation or changed count means the rows changed identity or
        // length, so the table must reload from the list.
        if coordinator.syncRows(totalCount: totalCount, epoch: epoch) {
            tableView.reloadData()
        }

        coordinator.configureSorting(on: tableView, sort: sort)
        coordinator.applySelection(selection, to: tableView)
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
        private var sortingEnabled: Bool
        private let libraryStore: LibraryStore
        private let runner: StorageActionRunner
        private var imageStore: ImageStore
        private var outboxStore: OutboxStore
        weak var tableView: NSTableView?

        private var rootEpoch: LoadEpoch?
        private var rootCount = 0

        /// True while we push selection into the table view, so the
        /// resulting `selectionDidChange` delegate callback doesn't write the
        /// same value back through the binding (and fight the next update).
        private var applyingSelection = false

        init(
            list: StorageList,
            selection: Binding<Set<String>>,
            sort: Binding<BridgeStorageSort>,
            inspectorPresented: Binding<Bool>,
            sortingEnabled: Bool,
            libraryStore: LibraryStore,
            runner: StorageActionRunner,
            imageStore: ImageStore,
            outboxStore: OutboxStore,
        ) {
            self.list = list
            self.selection = selection
            self.sort = sort
            self.inspectorPresented = inspectorPresented
            self.sortingEnabled = sortingEnabled
            self.libraryStore = libraryStore
            self.runner = runner
            self.imageStore = imageStore
            self.outboxStore = outboxStore
        }

        func update(
            list: StorageList,
            imageStore: ImageStore,
            outboxStore: OutboxStore,
            sortingEnabled: Bool,
        ) {
            self.list = list
            self.imageStore = imageStore
            self.outboxStore = outboxStore
            self.sortingEnabled = sortingEnabled
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

        // MARK: Lazy loading

        /// Kick off the page holding `position` if its id isn't loaded yet.
        /// `loadPage` coalesces concurrent asks for the same page, so calling
        /// this from every visible row issues at most one fetch per page.
        private func ensureLoaded(positionOf position: Int) {
            guard list.idAt(position) == nil else {
                return
            }
            let currentList = list
            Task { @MainActor in
                await currentList.loadPage(containing: position)
                // Only the still-mounted list should drive a reload; a tab
                // switch swaps the list before this resolves.
                guard currentList === self.list else {
                    return
                }
                self.tableView?.reloadData()
                self.applySelection(
                    self.selection.wrappedValue,
                    to: self.tableView
                )
            }
        }
    }
}

// MARK: - Sorting & selection

extension StorageTableView.Coordinator {
    func tableView(
        _ tableView: NSTableView,
        sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]
    ) {
        guard sortingEnabled,
            let descriptor = tableView.sortDescriptors.first,
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

    func configureSorting(
        on tableView: NSTableView,
        sort: BridgeStorageSort
    ) {
        for column in StorageTableColumn.allCases {
            guard
                let tableColumn = tableView.tableColumn(
                    withIdentifier: NSUserInterfaceItemIdentifier(
                        column.rawValue
                    )
                )
            else { continue }
            tableColumn.sortDescriptorPrototype =
                sortingEnabled
                ? column.sortField.map { _ in
                    NSSortDescriptor(key: column.rawValue, ascending: true)
                }
                : nil
        }
        guard sortingEnabled else {
            if !tableView.sortDescriptors.isEmpty {
                tableView.sortDescriptors = []
            }
            return
        }
        applySortIndicator(to: tableView, sort: sort)
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
            && selection.wrappedValue == [releaseId]
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
        ensureLoaded(positionOf: row)
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
    /// the storage badge reads `OutboxStore`).
    @ViewBuilder
    private func content(
        forRow row: Int,
        column: StorageTableColumn
    ) -> some View {
        releaseCell(row: row, column: column)
            .environment(imageStore)
            .environment(outboxStore)
    }

    private func releaseCell(
        row: Int,
        column: StorageTableColumn
    ) -> AnyView {
        guard let id = list.idAt(row) else {
            // The page covering this row hasn't loaded yet; the load is
            // already in flight from `ensureLoaded`. A standing bar shows
            // until the id arrives and the row reloads.
            return AnyView(StorageRowPlaceholderCell(column: column))
        }
        guard let summary = libraryStore.releaseSummaries[id],
            let album = libraryStore.albumSummaries[summary.albumId]
        else {
            // The id loaded but its summary/album didn't intern — a store
            // inconsistency. Surface it; show the placeholder over a crash.
            logger.error(
                "Loaded release \(id) has no interned summary or album"
            )
            return AnyView(StorageRowPlaceholderCell(column: column))
        }
        return AnyView(
            StorageReleaseCell(
                release: summary,
                album: album,
                column: column
            )
        )
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
        let targets = menuTargets(forClickedRelease: clickedId)
        guard !targets.isEmpty else { return }

        // A release mid-transition (uploading, pinning, or becoming local) can't
        // start another storage action (those race the transition), so the only
        // action it offers is cancelling — in every tab. Uploads live in the
        // outbox; foreground transfers (pin/unpin/make-local) set `transfer`.
        let transitioning = targets.filter { id in
            outboxStore.isTransitioning(forRelease: id)
                || libraryStore.releaseSummaries[id]?.transfer != nil
        }
        if !transitioning.isEmpty {
            let cancellable = transitioning.filter { id in
                if let observation = outboxStore.storageUploadObservation(
                    forRelease: id
                ) {
                    return observation.canCancel
                }
                return libraryStore.releaseSummaries[id]?.transfer != nil
            }
            if !cancellable.isEmpty {
                addMenuItem(
                    to: menu,
                    title: String(localized: "Cancel"),
                    action: #selector(cancelTransitionsAction(_:)),
                    symbol: "xmark.circle",
                    representedObject: cancellable
                )
            }
            return
        }
        for action in intersectedActions(of: targets) {
            addMenuItem(
                to: menu,
                title: action.label,
                action: #selector(runStorageAction(_:)),
                symbol: action.systemImage,
                representedObject: StorageActionInvocation(
                    action: action,
                    releaseIds: targets
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
            representedObject: targets
        )
        addMenuItem(
            to: menu,
            title: String(localized: "Save As…"),
            action: #selector(runSaveAsAction(_:)),
            symbol: "square.and.arrow.down",
            representedObject: targets
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

    /// If the clicked release is already selected, act on the whole selection;
    /// otherwise act on and select that release.
    private func menuTargets(forClickedRelease clickedId: String) -> [String] {
        if selection.wrappedValue.contains(clickedId) {
            return Array(selection.wrappedValue)
        }
        selection.wrappedValue = [clickedId]
        applySelection([clickedId], to: tableView)
        return [clickedId]
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

    /// Storage actions every targeted release allows, preserving the order the
    /// core emits them in. Single target → that release's actions; multi-select
    /// → the intersection so the menu only offers transitions applicable to
    /// all. Callers handle the uploading case before reaching here (an uploading
    /// release offers only "Cancel Upload").
    private func intersectedActions(
        of releaseIds: [String]
    ) -> [BridgeReleaseStorageAction] {
        let perRelease = releaseIds.map { id in
            Set(libraryStore.releaseSummaries[id]?.storageActions ?? [])
        }
        guard let common = perRelease.reduce(nil, intersectActions) else {
            return []
        }
        // Order off the first target's release; every action in `common` is by
        // construction present in it.
        let order =
            releaseIds.first
            .flatMap { libraryStore.releaseSummaries[$0]?.storageActions }
            ?? []
        return order.filter(common.contains)
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

/// Fold helper for `reduce`: the running intersection, seeded with the first
/// set (so the result is the intersection of all, not the empty set).
private func intersectActions(
    _ acc: Set<BridgeReleaseStorageAction>?,
    _ next: Set<BridgeReleaseStorageAction>
) -> Set<BridgeReleaseStorageAction> {
    guard let acc else { return next }
    return acc.intersection(next)
}
