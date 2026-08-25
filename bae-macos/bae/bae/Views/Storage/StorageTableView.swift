import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("StorageTable")

// MARK: - Items

/// A root row: one release at a fixed list position. Reference identity is
/// stable across `reloadData`, so the outline view preserves expansion and
/// selection when the data source reloads. The release id is resolved lazily
/// through `list.idAt(index)` — the item is just the position.
private final class StorageReleaseItem {
    let index: Int
    init(index: Int) { self.index = index }
}

/// A child row under an expanded release: one file. Carries the file payload
/// and its owning release id (the file columns render directly off the file;
/// the release id is only used to map a file selection back to its release).
private final class StorageFileItem {
    let file: BridgeFile
    let releaseId: String
    init(file: BridgeFile, releaseId: String) {
        self.file = file
        self.releaseId = releaseId
    }
}

// MARK: - Representable

/// An `NSOutlineView`-backed storage table: the outline view handles the table
/// mechanics (resizable / reorderable columns, header sort indicators,
/// disclosure triangles, shift/cmd selection) with SwiftUI-hosted cell content
/// so covers and the storage badge reuse `ImageView` / `Theme` instead of
/// reimplementing image loading in AppKit.
///
/// Releases are root rows (lazy-loaded in batches by position); expanding a
/// release shows its files (loaded on demand). Selection binds to release ids;
/// the column sort maps to a `BridgeStorageSort` and goes back through the
/// caller's `rebuildList()` (server-side — the header indicator only reflects
/// it).
struct StorageTableView: NSViewRepresentable {
    let list: StorageList
    @Binding
    var selection: Set<String>
    @Binding
    var sort: BridgeStorageSort
    let sortingEnabled: Bool
    let libraryStore: LibraryStore
    let library: Library
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
            sortingEnabled: sortingEnabled,
            libraryStore: libraryStore,
            library: library,
            runner: runner,
            imageStore: imageStore,
            outboxStore: outboxStore,
        )
    }

    func makeNSView(context: Context) -> NSScrollView {
        let coordinator = context.coordinator
        let outlineView = NSOutlineView()
        outlineView.dataSource = coordinator
        outlineView.delegate = coordinator
        outlineView.menu = coordinator.makeContextMenu()
        outlineView.allowsMultipleSelection = true
        outlineView.allowsColumnReordering = true
        outlineView.allowsColumnResizing = true
        outlineView.usesAlternatingRowBackgroundColors = true
        outlineView.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        outlineView.indentationPerLevel = 16
        outlineView.rowHeight = 28
        outlineView.style = .inset

        for column in StorageTableColumn.allCases {
            let tableColumn = NSTableColumn(
                identifier: NSUserInterfaceItemIdentifier(column.rawValue)
            )
            tableColumn.title = column.title
            tableColumn.width = column.width
            tableColumn.minWidth = column.minWidth
            tableColumn.isEditable = false
            outlineView.addTableColumn(tableColumn)
        }
        // First column carries the disclosure triangle.
        outlineView.outlineTableColumn = outlineView.tableColumn(
            withIdentifier: NSUserInterfaceItemIdentifier(
                StorageTableColumn.album.rawValue
            )
        )
        coordinator.configureSorting(on: outlineView, sort: sort)

        let scrollView = NSScrollView()
        scrollView.documentView = outlineView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        coordinator.outlineView = outlineView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let outlineView = scrollView.documentView as? NSOutlineView else {
            logger.error(
                "Storage scroll view is missing its outline document view"
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

        // A new list instance, a new generation, or a changed count means the
        // root rows changed identity or length — rebuild the cached items and
        // reload from scratch.
        if coordinator.syncRootItems(totalCount: totalCount, epoch: epoch) {
            outlineView.reloadData()
        }

        coordinator.configureSorting(on: outlineView, sort: sort)
        coordinator.applySelection(selection, to: outlineView)
    }
}

// MARK: - Coordinator

extension StorageTableView {
    @MainActor
    final class Coordinator: NSObject, NSOutlineViewDataSource,
        NSOutlineViewDelegate
    {
        private(set) var list: StorageList
        private let selection: Binding<Set<String>>
        private let sort: Binding<BridgeStorageSort>
        private var sortingEnabled: Bool
        private let libraryStore: LibraryStore
        private let library: Library
        private let runner: StorageActionRunner
        private var imageStore: ImageStore
        private var outboxStore: OutboxStore
        weak var outlineView: NSOutlineView?

        /// Cached root items, one per position, reused across `reloadData` so
        /// the outline view keeps expansion and selection. Rebuilt only when
        /// the list epoch or count changes.
        private var rootItems: [StorageReleaseItem] = []
        private var rootEpoch: LoadEpoch?
        private var rootCount = 0

        /// Stable file-row items per release, keyed by release id. The outline
        /// view identifies items by object identity, so an expanded release MUST
        /// hand back the same `StorageFileItem` instances across every reload —
        /// otherwise AppKit treats each `reloadData` as a fresh set of children
        /// and re-inserts/animates the expanded rows on every reload. Built once
        /// from a release's loaded files; dropped when its detail reloads (on
        /// expand) or the root list changes.
        private var fileItemsByRelease: [String: [StorageFileItem]] = [:]
        private var detailTasksByRelease: [String: Task<Void, Never>] = [:]

        /// True while we push selection into the outline view, so the
        /// resulting `selectionDidChange` delegate callback doesn't write the
        /// same value back through the binding (and fight the next update).
        private var applyingSelection = false

        init(
            list: StorageList,
            selection: Binding<Set<String>>,
            sort: Binding<BridgeStorageSort>,
            sortingEnabled: Bool,
            libraryStore: LibraryStore,
            library: Library,
            runner: StorageActionRunner,
            imageStore: ImageStore,
            outboxStore: OutboxStore,
        ) {
            self.list = list
            self.selection = selection
            self.sort = sort
            self.sortingEnabled = sortingEnabled
            self.libraryStore = libraryStore
            self.library = library
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
            if self.list !== list {
                cancelDetailSubscriptions()
            }
            self.list = list
            self.imageStore = imageStore
            self.outboxStore = outboxStore
            self.sortingEnabled = sortingEnabled
        }

        /// Rebuild the cached root items when the list's epoch or count
        /// changes. Returns whether a reload is needed.
        func syncRootItems(totalCount: Int, epoch: LoadEpoch) -> Bool {
            guard rootEpoch != epoch || rootCount != totalCount else {
                return false
            }
            rootEpoch = epoch
            rootCount = totalCount
            rootItems = (0..<totalCount).map { StorageReleaseItem(index: $0) }
            // Positions (and which release sits at each) may have moved, so the
            // cached file items no longer belong to these rows.
            fileItemsByRelease.removeAll()
            cancelDetailSubscriptions()
            return true
        }

        private func cancelDetailSubscriptions() {
            for task in detailTasksByRelease.values {
                task.cancel()
            }
            detailTasksByRelease.removeAll()
        }

        /// Stable file-row items for a release, cached so the outline view sees
        /// the same child objects across reloads. Returns `[]` (without caching)
        /// until the release's detail is loaded, so the cache fills the first
        /// time the files are present and stays stable thereafter.
        private func fileItems(forReleaseId id: String) -> [StorageFileItem] {
            if let cached = fileItemsByRelease[id] {
                return cached
            }
            guard let files = libraryStore.releaseDetails[id]?.files else {
                return []
            }
            let items = files.map { StorageFileItem(file: $0, releaseId: id) }
            fileItemsByRelease[id] = items
            return items
        }

        // MARK: Data source

        func outlineView(
            _ outlineView: NSOutlineView,
            numberOfChildrenOfItem item: Any?
        ) -> Int {
            switch item {
            case nil:
                return rootItems.count
            case let release as StorageReleaseItem:
                guard let id = list.idAt(release.index) else {
                    return 0
                }
                // 0 until the release's detail loads on expand; the disclosure
                // triangle still shows because `isItemExpandable` is true. A
                // release always has files, so 0 here means "not loaded yet".
                return fileItems(forReleaseId: id).count
            default:
                return 0
            }
        }

        func outlineView(
            _ outlineView: NSOutlineView,
            child index: Int,
            ofItem item: Any?
        ) -> Any {
            switch item {
            case nil:
                let release = rootItems[index]
                ensureLoaded(positionOf: release.index)
                return release
            case let release as StorageReleaseItem:
                // AppKit only asks for a child when `numberOfChildrenOfItem`
                // reported > 0, which requires the id and loaded files — so
                // both resolve here. If they don't (a page unloaded mid-query),
                // there's no file to return; log and hand back the release so
                // the table stays consistent rather than crashing. The cached
                // items keep object identity stable across reloads.
                guard let id = list.idAt(release.index) else {
                    logger.error(
                        "No release id for child \(index) of row \(release.index)"
                    )
                    return release
                }
                let items = fileItems(forReleaseId: id)
                guard index < items.count else {
                    logger.error(
                        "No file at child \(index) of release row \(release.index)"
                    )
                    return release
                }
                return items[index]
            default:
                // Only releases report children, so AppKit never asks any
                // other item for one — an unexpected parent is a broken
                // data-source state.
                logger.error(
                    "Unexpected parent item type requesting child \(index)"
                )
                return rootItems.indices.contains(index)
                    ? rootItems[index] : StorageReleaseItem(index: index)
            }
        }

        func outlineView(
            _ outlineView: NSOutlineView,
            isItemExpandable item: Any
        ) -> Bool {
            // Every release has files; file rows are leaves.
            item is StorageReleaseItem
        }

        // MARK: Lazy loading

        /// Kick off the page holding `position` if its id isn't loaded yet.
        /// `loadPage` coalesces concurrent asks for the same page, so calling
        /// this from every visible row issues at most one fetch per page.
        /// Reloading after the fetch preserves expansion/selection because the
        /// root items are stable.
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
                self.outlineView?.reloadData()
                self.applySelection(
                    self.selection.wrappedValue,
                    to: self.outlineView
                )
            }
        }

        // MARK: Expansion

        func outlineViewItemWillExpand(_ notification: Notification) {
            guard
                let release = notification.userInfo?["NSObject"]
                    as? StorageReleaseItem
            else {
                logger.error("Expand notification carried no release item")
                return
            }
            guard let id = list.idAt(release.index) else {
                logger.error(
                    "Expanding row \(release.index) resolved no release id; skipping file load"
                )
                return
            }
            guard detailTasksByRelease[id] == nil else {
                return
            }
            let currentList = list
            detailTasksByRelease[id] = Task { @MainActor in
                await self.libraryStore.observeReleaseDetail(
                    releaseId: id,
                    library: self.library,
                    onValue: {
                        guard currentList === self.list else {
                            return
                        }
                        self.fileItemsByRelease[id] = nil
                        self.outlineView?
                            .reloadItem(
                                release,
                                reloadChildren: true
                            )
                    }
                )
                self.detailTasksByRelease[id] = nil
            }
        }

        func outlineViewItemDidCollapse(_ notification: Notification) {
            guard
                let release = notification.userInfo?["NSObject"]
                    as? StorageReleaseItem,
                let id = list.idAt(release.index)
            else {
                return
            }
            detailTasksByRelease.removeValue(forKey: id)?.cancel()
            fileItemsByRelease.removeValue(forKey: id)
        }
    }
}

// MARK: - Sorting & selection

extension StorageTableView.Coordinator {
    func outlineView(
        _ outlineView: NSOutlineView,
        sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]
    ) {
        guard sortingEnabled,
            let descriptor = outlineView.sortDescriptors.first,
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
        on outlineView: NSOutlineView,
        sort: BridgeStorageSort
    ) {
        for column in StorageTableColumn.allCases {
            guard
                let tableColumn = outlineView.tableColumn(
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
            if !outlineView.sortDescriptors.isEmpty {
                outlineView.sortDescriptors = []
            }
            return
        }
        applySortIndicator(to: outlineView, sort: sort)
    }

    /// Reflect the active `BridgeStorageSort` in the header indicator
    /// without re-triggering `sortDescriptorsDidChange` (set the array
    /// directly rather than mutating the column).
    func applySortIndicator(
        to outlineView: NSOutlineView,
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
        if outlineView.sortDescriptors.first != descriptor {
            outlineView.sortDescriptors = [descriptor]
        }
    }

    // MARK: Selection

    /// The release id an outline item belongs to: a release row resolves
    /// through `list.idAt(index)`, a file row carries its owning release id.
    func releaseId(for item: Any) -> String? {
        if let release = item as? StorageReleaseItem {
            return list.idAt(release.index)
        }
        if let file = item as? StorageFileItem {
            return file.releaseId
        }
        return nil
    }

    func outlineViewSelectionDidChange(_ notification: Notification) {
        guard !applyingSelection,
            let outlineView = notification.object as? NSOutlineView
        else {
            return
        }
        var ids: Set<String> = []
        for row in outlineView.selectedRowIndexes {
            guard let item = outlineView.item(atRow: row) else {
                logger.warning("Selected row \(row) resolved no item")
                continue
            }
            if let id = releaseId(for: item) {
                ids.insert(id)
            }
        }
        if ids != selection.wrappedValue {
            selection.wrappedValue = ids
        }
    }

    /// Push the bound release-id selection into the outline view's row
    /// selection, mapping each id to its loaded position. Guarded so the
    /// resulting delegate callback doesn't echo back through the binding.
    func applySelection(
        _ ids: Set<String>,
        to outlineView: NSOutlineView?
    ) {
        guard let outlineView else { return }
        var rows = IndexSet()
        for id in ids {
            // `position(of:)` reads loaded segments, which can briefly hold
            // a position past freshly-rebuilt `rootItems` while a new live page
            // is being installed. A selected id whose row isn't loaded can't
            // be highlighted yet; skip it.
            guard let position = list.position(of: id),
                position < rootItems.count
            else {
                logger.debug(
                    "Selected release \(id) has no loaded row to highlight"
                )
                continue
            }
            let row = outlineView.row(forItem: rootItems[position])
            if row >= 0 {
                rows.insert(row)
            }
        }
        guard rows != outlineView.selectedRowIndexes else {
            return
        }
        applyingSelection = true
        outlineView.selectRowIndexes(rows, byExtendingSelection: false)
        applyingSelection = false
    }
}

// MARK: - Cell views

extension StorageTableView.Coordinator {
    func outlineView(
        _ outlineView: NSOutlineView,
        viewFor tableColumn: NSTableColumn?,
        item: Any
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
        let cell = dequeueCell(outlineView, column: column)
        cell.host(content(for: item, column: column))
        return cell
    }

    private func dequeueCell(
        _ outlineView: NSOutlineView,
        column: StorageTableColumn
    ) -> HostingTableCell {
        let identifier = NSUserInterfaceItemIdentifier(column.rawValue)
        if let reused = outlineView.makeView(
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
        for item: Any,
        column: StorageTableColumn
    ) -> some View {
        cellBody(for: item, column: column)
            .environment(imageStore)
            .environment(outboxStore)
    }

    @ViewBuilder
    private func cellBody(
        for item: Any,
        column: StorageTableColumn
    ) -> some View {
        if let release = item as? StorageReleaseItem {
            releaseCell(release, column: column)
        }
        else if let file = item as? StorageFileItem {
            StorageFileCell(file: file.file, column: column)
        }
    }

    private func releaseCell(
        _ release: StorageReleaseItem,
        column: StorageTableColumn
    ) -> AnyView {
        guard let id = list.idAt(release.index) else {
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
        guard let outlineView else { return }
        let clickedRow = outlineView.clickedRow
        guard clickedRow >= 0,
            let item = outlineView.item(atRow: clickedRow)
        else {
            return
        }

        // Right-click acts on the selection when the clicked row is part of it,
        // otherwise on just that row (and selects it).
        let targets = menuTargets(forClicked: item)
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

    /// Release ids the menu acts on. A file row maps to its owning release. If
    /// the clicked release is already selected, act on the whole selection;
    /// otherwise act on (and select) just that release.
    private func menuTargets(forClicked item: Any) -> [String] {
        guard let clickedId = releaseId(for: item) else {
            logger.error("Right-clicked row resolved no release id")
            return []
        }

        if selection.wrappedValue.contains(clickedId) {
            return Array(selection.wrappedValue)
        }
        selection.wrappedValue = [clickedId]
        applySelection([clickedId], to: outlineView)
        return [clickedId]
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
