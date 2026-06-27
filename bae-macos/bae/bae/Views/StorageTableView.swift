import AppKit
import OSLog
import SwiftUI

private let logger = Logger.bae("StorageTable")

/// How many rows the table fetches per `loadRange` call. A row whose id isn't
/// loaded yet kicks off the batch covering its position; `loadRange` coalesces
/// concurrent asks for the same batch into one fetch.
private let storageLoadBatchSize = 100

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

// MARK: - Columns

/// Identifiers and seed metadata for the six storage columns. The widths are
/// the initial layout only; the outline view owns resize/reorder at runtime.
/// The storage column has no sort field — storage state isn't a sort key the
/// core exposes.
private enum StorageTableColumn: String, CaseIterable {
    case album
    case artist
    case format
    case storage
    case files
    case size

    /// One column's metadata, kept in a single spec so the per-property
    /// accessors don't fan out into four parallel switches over the same cases.
    private struct Spec {
        let title: String
        let width: CGFloat
        let minWidth: CGFloat
        let sortField: BridgeStorageSortField?
    }

    private var spec: Spec {
        switch self {
        case .album:
            Spec(
                title: String(localized: "Album"),
                width: 280,
                minWidth: 160,
                sortField: .albumTitle
            )
        case .artist:
            Spec(
                title: String(localized: "Artist"),
                width: 200,
                minWidth: 100,
                sortField: .artistNames
            )
        case .format:
            Spec(
                title: String(localized: "Format"),
                width: 80,
                minWidth: 60,
                sortField: .format
            )
        case .storage:
            Spec(
                title: String(localized: "Storage"),
                width: 140,
                minWidth: 100,
                sortField: nil
            )
        case .files:
            Spec(
                title: String(localized: "Files"),
                width: 60,
                minWidth: 44,
                sortField: .fileCount
            )
        case .size:
            Spec(
                title: String(localized: "Size"),
                width: 100,
                minWidth: 70,
                sortField: .totalSize
            )
        }
    }

    var title: String { spec.title }
    var width: CGFloat { spec.width }
    var minWidth: CGFloat { spec.minWidth }
    var sortField: BridgeStorageSortField? { spec.sortField }
}

/// `NSSortDescriptor.key` values for the sortable columns: the column raw
/// value, mapped back to a `BridgeStorageSortField` when the user clicks a
/// header.
private func sortField(forDescriptorKey key: String) -> BridgeStorageSortField?
{
    StorageTableColumn(rawValue: key)?.sortField
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
    let libraryStore: LibraryStore
    let library: Library
    let runner: StorageActionRunner

    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(OutboxStore.self)
    private var outboxStore

    func makeCoordinator() -> Coordinator {
        Coordinator(
            list: list,
            selection: $selection,
            sort: $sort,
            libraryStore: libraryStore,
            library: library,
            runner: runner,
            mediaPaths: mediaPaths,
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
            if column.sortField != nil {
                tableColumn.sortDescriptorPrototype = NSSortDescriptor(
                    key: column.rawValue,
                    ascending: true
                )
            }
            outlineView.addTableColumn(tableColumn)
        }
        // First column carries the disclosure triangle.
        outlineView.outlineTableColumn = outlineView.tableColumn(
            withIdentifier: NSUserInterfaceItemIdentifier(
                StorageTableColumn.album.rawValue
            )
        )
        coordinator.applySortIndicator(to: outlineView, sort: sort)

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
        // re-invoke `updateNSView` when the list grows, is invalidated, or is
        // swapped for a fresh instance on a sort/filter change.
        let totalCount = list.totalCount
        let epoch = list.loadEpoch

        coordinator.update(
            list: list,
            mediaPaths: mediaPaths,
            outboxStore: outboxStore,
        )

        // A new list instance, a new generation, or a changed count means the
        // root rows changed identity or length — rebuild the cached items and
        // reload from scratch.
        if coordinator.syncRootItems(totalCount: totalCount, epoch: epoch) {
            outlineView.reloadData()
        }

        coordinator.applySortIndicator(to: outlineView, sort: sort)
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
        private let libraryStore: LibraryStore
        private let library: Library
        private let runner: StorageActionRunner
        private var mediaPaths: MediaPaths
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

        /// True while we push selection into the outline view, so the
        /// resulting `selectionDidChange` delegate callback doesn't write the
        /// same value back through the binding (and fight the next update).
        private var applyingSelection = false

        init(
            list: StorageList,
            selection: Binding<Set<String>>,
            sort: Binding<BridgeStorageSort>,
            libraryStore: LibraryStore,
            library: Library,
            runner: StorageActionRunner,
            mediaPaths: MediaPaths,
            outboxStore: OutboxStore,
        ) {
            self.list = list
            self.selection = selection
            self.sort = sort
            self.libraryStore = libraryStore
            self.library = library
            self.runner = runner
            self.mediaPaths = mediaPaths
            self.outboxStore = outboxStore
        }

        func update(
            list: StorageList,
            mediaPaths: MediaPaths,
            outboxStore: OutboxStore,
        ) {
            self.list = list
            self.mediaPaths = mediaPaths
            self.outboxStore = outboxStore
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
            return true
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

        /// Kick off the batch covering `position` if its id isn't loaded yet.
        /// `loadRange` coalesces concurrent asks for the same batch, so calling
        /// this from every visible row issues at most one fetch per batch.
        /// Reloading after the fetch preserves expansion/selection because the
        /// root items are stable.
        private func ensureLoaded(positionOf position: Int) {
            guard list.idAt(position) == nil else {
                return
            }
            let batchStart =
                (position / storageLoadBatchSize) * storageLoadBatchSize
            let currentList = list
            Task { @MainActor in
                await currentList.loadRange(
                    offset: batchStart,
                    limit: storageLoadBatchSize
                )
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
            // `reloadItem(_:reloadChildren:)` below re-fires this notification
            // while AppKit rebuilds the expanded row's children. Once the detail
            // is loaded the children resolve directly, so bail on that re-entrant
            // call — otherwise the load + reload repeats forever, re-hosting the
            // row's cells (the cover's load task restarts every pass, so its
            // spinner never resolves) and collapsing/re-expanding the children
            // (the rows jump).
            guard libraryStore.releaseDetails[id] == nil else {
                return
            }
            let currentList = list
            Task { @MainActor in
                await self.libraryStore.loadReleaseDetail(
                    releaseId: id,
                    library: self.library
                )
                guard currentList === self.list else {
                    return
                }
                // Only refresh children once the detail actually loaded. If it
                // didn't, there are no file rows to show and reloading would
                // re-enter this handler with the detail still absent — the same
                // loop, never breaking because the guard above never trips.
                guard libraryStore.releaseDetails[id] != nil else {
                    logger.warning(
                        "Detail failed to load for expanded release \(id); leaving it without file rows"
                    )
                    return
                }
                // Rebuild this release's stable file items from the freshly
                // loaded detail, then let the outline view pick them up once.
                self.fileItemsByRelease[id] = nil
                self.outlineView?.reloadItem(release, reloadChildren: true)
            }
        }
    }
}

// MARK: - Sorting & selection

extension StorageTableView.Coordinator {
    func outlineView(
        _ outlineView: NSOutlineView,
        sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]
    ) {
        guard
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
            // a position past the freshly-rebuilt `rootItems` right after an
            // `invalidate()` shrinks the count (stale segments outlive the
            // rebuild). A selected id whose row isn't currently loaded can't
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
    /// hosted cell views read at the leaf (`ImageView` needs `MediaPaths`;
    /// the storage badge reads `OutboxStore`).
    @ViewBuilder
    private func content(
        for item: Any,
        column: StorageTableColumn
    ) -> some View {
        cellBody(for: item, column: column)
            .environment(mediaPaths)
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

        // A release mid-transition (uploading, pinning, or unmanaging) can't be
        // managed/pinned/unmanaged (those race the transition), so the only
        // action it offers is cancelling — in every tab. Uploads live in the
        // outbox; foreground transfers (pin/unpin/unmanage) set `transfer`.
        let transitioning = targets.filter { id in
            outboxStore.isUploading(forRelease: id)
                || libraryStore.releaseSummaries[id]?.transfer != nil
        }
        if !transitioning.isEmpty {
            addMenuItem(
                to: menu,
                title: String(localized: "Cancel"),
                action: #selector(cancelTransitionsAction(_:)),
                symbol: "xmark.circle",
                representedObject: transitioning
            )
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
    }

    /// Build a context-menu item targeting this coordinator and add it to the
    /// menu. Shared by the cancel-upload and storage-action branches.
    private func addMenuItem(
        to menu: NSMenu,
        title: String,
        action: Selector,
        symbol: String,
        representedObject: Any,
        isEnabled: Bool = true
    ) {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.image = NSImage(
            systemSymbolName: symbol,
            accessibilityDescription: nil
        )
        item.target = self
        item.representedObject = representedObject
        item.isEnabled = isEnabled
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

// MARK: - Hosting cell

/// An `NSTableCellView` that draws its content through a single reused
/// `NSHostingView`. Dequeued per column so SwiftUI cell views (covers, badges,
/// labels) render inside the `NSOutlineView`.
private final class HostingTableCell: NSTableCellView {
    private var hosting: NSHostingView<AnyView>?

    func host(_ content: some View) {
        let wrapped = AnyView(content)
        if let hosting {
            hosting.rootView = wrapped
            return
        }
        let view = NSHostingView(rootView: wrapped)
        view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: leadingAnchor),
            view.trailingAnchor.constraint(equalTo: trailingAnchor),
            view.topAnchor.constraint(equalTo: topAnchor),
            view.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        hosting = view
    }
}

// MARK: - Cell content

/// Release row content for one column. Reads the storage badge's `OutboxStore`
/// and the cover's `MediaPaths` at the leaf (injected on the hosted view).
private struct StorageReleaseCell: View {
    let release: ReleaseSummary
    let album: AlbumSummary
    let column: StorageTableColumn

    var body: some View {
        Group {
            switch column {
            case .album:
                HStack(spacing: 8) {
                    ImageView(imageRef: release.cover, pointSize: 24)
                        .frame(width: 24, height: 24)
                        .clipShape(RoundedRectangle(cornerRadius: 3))
                    Text(album.title).lineLimit(1)
                }
            case .artist:
                Text(album.artistNames).lineLimit(1)
            case .format:
                Text(release.format ?? "\u{2014}")
            case .storage:
                StorageStateLabel(release: release)
            case .files:
                Text(verbatim: release.fileCount.formatted())
                    .monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .trailing)
            case .size:
                Text(release.totalSizeText)
                    .monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
        }
        .frame(maxWidth: .infinity, alignment: cellAlignment(column))
        .padding(.horizontal, 4)
    }
}

/// The storage badge: an in-flight transfer or queued upload wins over the
/// resting storage state.
private struct StorageStateLabel: View {
    let release: ReleaseSummary
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        if let transfer = release.transfer {
            Label(transfer.label, systemImage: "arrow.down.circle")
                .foregroundStyle(.blue)
                .lineLimit(1)
        }
        else if let progress = outboxStore.progress(forRelease: release.id),
            let activity = progress.activity
        {
            uploadBadge(activity, remaining: Int(progress.pending))
        }
        else {
            switch release.storageState {
            case .local:
                Label("Unmanaged", systemImage: "folder").lineLimit(1)
            case .remote:
                if release.pinned {
                    Label("Pinned", systemImage: "pin.fill").lineLimit(1)
                }
                else {
                    Label("Cloud", systemImage: "cloud").lineLimit(1)
                }
            }
        }
    }

    /// The per-release upload badge: only a release with a file in flight reads
    /// as "Uploading"; one with work still waiting reads as "Queued", and one
    /// stalled on failures awaiting retry as "Retrying". `remaining` is the
    /// release's total unshipped file count.
    @ViewBuilder
    private func uploadBadge(
        _ activity: BridgeUploadActivity,
        remaining: Int
    ) -> some View {
        switch activity {
        case .uploading:
            Label("Uploading (\(remaining))", systemImage: "arrow.up.circle")
                .foregroundStyle(.orange)
                .lineLimit(1)
        case .queued:
            Label("Queued (\(remaining))", systemImage: "clock")
                .foregroundStyle(.secondary)
                .lineLimit(1)
        case .retrying:
            Label(
                "Retrying (\(remaining))",
                systemImage: "exclamationmark.triangle.fill"
            )
            .foregroundStyle(.red)
            .lineLimit(1)
        }
    }
}

/// File row content (a release's expanded child): filename in the outline
/// column, audio-format descriptor in Format, formatted size in Size. The
/// artist / storage / files columns stay blank for files.
private struct StorageFileCell: View {
    let file: BridgeFile
    let column: StorageTableColumn

    var body: some View {
        Group {
            switch column {
            case .album:
                Text(file.originalFilename)
                    .lineLimit(1)
                    .foregroundStyle(.secondary)
            case .format:
                // Audio files carry a label; non-audio files (images, cue)
                // have none, so their Format cell is simply empty.
                if let format = file.audioFormat {
                    Text(format.text)
                        .lineLimit(1)
                        .foregroundStyle(.secondary)
                }
            case .size:
                Text(file.fileSizeText)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            case .artist, .storage, .files:
                EmptyView()
            }
        }
        .frame(maxWidth: .infinity, alignment: cellAlignment(column))
        .padding(.horizontal, 4)
    }
}

/// Placeholder shown for a row whose page hasn't loaded yet. Only the first
/// column carries the standing bar; the rest stay empty.
private struct StorageRowPlaceholderCell: View {
    let column: StorageTableColumn

    var body: some View {
        Group {
            if column == .album {
                RoundedRectangle(cornerRadius: 3)
                    .fill(Color.gray.opacity(0.15))
                    .frame(height: 14)
            }
            else {
                EmptyView()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 4)
    }
}

/// Files and Size read right-aligned (numeric); the rest are leading.
private func cellAlignment(_ column: StorageTableColumn) -> Alignment {
    switch column {
    case .files, .size: .trailing
    default: .leading
    }
}
