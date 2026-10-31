import AppKit
import BaeKit
import SwiftUI

/// What the candidate list's menu shows.
struct CandidateListMenuState {
    let watchedFolders: [BridgeWatchedFolder]
    /// Roots with a refresh in flight; their Refresh entry is disabled.
    let refreshingFolders: Set<Data>
    /// Each root's scan state, by path.
    let scanStatuses: [Data: BridgeFolderScanStatus]
    /// Roots on a network volume, which are checked on a schedule rather than
    /// reported the moment they change.
    let networkFolders: Set<Data>
    /// Whether the queue has folder groups; without any, Expand All and
    /// Collapse All are disabled.
    let hasGroups: Bool
    let sortOrder: BridgeImportListOrder
    /// The entry Found's rows are narrowed to.
    let pendingFilter: BridgePendingFilter
    /// Every entry of Found's filter, in core's order, with how many rows
    /// each holds.
    let pendingFilterEntries: [BridgePendingFilterEntry]
    /// Whether the tab on show is Found, the one tab the filter applies to.
    let pendingFilterApplies: Bool

    /// Whether any root's last scan failed, which also marks the trigger.
    var hasFailedScan: Bool {
        scanStatuses.values.contains { status in
            if case .failed = status { return true }
            return false
        }
    }
}

/// What choosing each of the menu's items does.
struct CandidateListMenuActions {
    let onSetSortOrder: (BridgeImportListOrder) -> Void
    let onSetPendingFilter: (BridgePendingFilter) -> Void
    let onAddFolder: () -> Void
    /// Fold every folder group in the queue open (`true`) or shut (`false`).
    let onSetAllGroupsExpanded: (_ expanded: Bool) -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    /// Stop watching `path`.
    let onRemoveFolder: (_ path: Data) -> Void
}

/// The candidate list's menu: sort order, Found's filter, watched folders,
/// and expanding or collapsing every folder group.
///
/// An AppKit menu, built from the state as it stands each time it opens.
/// While it is open, a change to the filter's counts retitles and re-enables
/// its filter items where they stand, so the counts on screen are the list's
/// latest and the menu stays open under the pointer. Everything else it shows
/// is taken up the next time it opens.
@MainActor
final class CandidateListMenuController: NSObject, NSMenuDelegate {
    let menu = NSMenu()
    private(set) var state: CandidateListMenuState
    var actions: CandidateListMenuActions
    /// The Filter section's header, which its items follow.
    private let filterHeader = NSMenuItem.sectionHeader(
        title: String(localized: "Filter")
    )
    /// The filter's items in the menu as last built, in core's order, each
    /// with the entry it shows.
    private var filterItems: [(filter: BridgePendingFilter, item: NSMenuItem)] =
        []

    init(state: CandidateListMenuState, actions: CandidateListMenuActions) {
        self.state = state
        self.actions = actions
        super.init()
        menu.delegate = self
        menu.autoenablesItems = false
        rebuild()
    }

    /// Show `state`: the filter's items take it at once, the rest when the
    /// menu next opens.
    func show(_ state: CandidateListMenuState) {
        self.state = state
        updateFilterItems()
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        guard menu === self.menu else { return }
        rebuild()
    }

    // MARK: Building

    private func rebuild() {
        menu.removeAllItems()
        var items: [NSMenuItem] = []
        items.append(.sectionHeader(title: String(localized: "Sort")))
        for (title, order) in Self.sortOrders {
            let item = ActionMenuItem(title: title) { [weak self] in
                self?.actions.onSetSortOrder(order)
            }
            item.state = order == state.sortOrder ? .on : .off
            items.append(item)
        }
        items.append(.separator())
        items.append(filterHeader)
        filterItems = state.pendingFilterEntries.map {
            ($0.filter, filterItem($0))
        }
        items.append(contentsOf: filterItems.map(\.item))
        items.append(.separator())
        items.append(.sectionHeader(title: String(localized: "Folders")))
        items.append(
            ActionMenuItem(
                title: String(localized: "Add a Folder\u{2026}"),
                symbol: "plus"
            ) { [weak self] in
                self?.actions.onAddFolder()
            }
        )
        for folder in state.watchedFolders {
            items.append(folderItem(folder))
        }
        items.append(.separator())
        items.append(.sectionHeader(title: String(localized: "Groups")))
        for (title, symbol, expanded) in [
            (String(localized: "Expand All"), "chevron.down", true),
            (String(localized: "Collapse All"), "chevron.right", false),
        ] {
            let item = ActionMenuItem(title: title, symbol: symbol) {
                [weak self] in
                self?.actions.onSetAllGroupsExpanded(expanded)
            }
            item.isEnabled = state.hasGroups
            items.append(item)
        }
        menu.items = items
    }

    private static let sortOrders: [(String, BridgeImportListOrder)] = [
        (String(localized: "Newest First"), .newestFirst),
        (String(localized: "Oldest First"), .oldestFirst),
        (String(localized: "Folder Name (A–Z)"), .pathAscending),
        (String(localized: "Folder Name (Z–A)"), .pathDescending),
    ]

    /// One entry of Found's filter, checked when it is the chosen one.
    /// Choosing the chosen entry again changes nothing.
    private func filterItem(_ entry: BridgePendingFilterEntry) -> NSMenuItem {
        let filter = entry.filter
        let item = ActionMenuItem(title: "") { [weak self] in
            guard let self, filter != state.pendingFilter else { return }
            actions.onSetPendingFilter(filter)
        }
        configure(item, for: entry)
        return item
    }

    /// Title, check and enabled state from `entry`: an entry holding no row
    /// cannot be chosen, one already chosen stays checked, and none can be
    /// chosen off Found.
    private func configure(
        _ item: NSMenuItem,
        for entry: BridgePendingFilterEntry
    ) {
        item.title = entry.filter.label(count: entry.count)
        item.state = entry.filter == state.pendingFilter ? .on : .off
        item.isEnabled = state.pendingFilterApplies && entry.selectable
    }

    /// Retitle, recheck and re-enable the built filter items from the
    /// state's entries. Core lists every entry, in one order, on every
    /// delivery, so the entries line up with the items built from an earlier
    /// one; the items built before the first delivery, of which there are
    /// none, are replaced where they stand.
    private func updateFilterItems() {
        let entries = state.pendingFilterEntries
        guard entries.map(\.filter) == filterItems.map(\.filter) else {
            replaceFilterItems()
            return
        }
        for ((_, item), entry) in zip(filterItems, entries) {
            configure(item, for: entry)
        }
    }

    /// Put items for the state's entries after the Filter header, in place of
    /// the ones there.
    private func replaceFilterItems() {
        for (_, item) in filterItems {
            menu.removeItem(item)
        }
        filterItems = state.pendingFilterEntries.map {
            ($0.filter, filterItem($0))
        }
        let header = menu.index(of: filterHeader)
        for (offset, (_, item)) in filterItems.enumerated() {
            menu.insertItem(item, at: header + 1 + offset)
        }
    }

    /// One root's entry: its scan state is the icon, and a failed scan's
    /// error is the tooltip.
    private func folderItem(_ folder: BridgeWatchedFolder) -> NSMenuItem {
        let item = NSMenuItem(
            title: folder.name,
            action: nil,
            keyEquivalent: ""
        )
        let onNetwork = state.networkFolders.contains(folder.path)
        let networkLine = onNetwork ? Self.networkLine() : nil
        switch state.scanStatuses[folder.path] {
        case .scanning:
            item.image = Self.symbol("progress.indicator")
            item.toolTip = networkLine
        case .failed(let error):
            item.image = Self.symbol(
                "exclamationmark.triangle.fill",
                color: NSColor(Theme.danger)
            )
            item.toolTip = [error, networkLine]
                .compactMap { $0 }
                .joined(separator: "\n")
        case .complete, nil:
            if onNetwork {
                item.image = Self.symbol(
                    "network",
                    color: .secondaryLabelColor
                )
            }
            item.toolTip = networkLine
        }
        let submenu = NSMenu()
        submenu.autoenablesItems = false
        let refreshing = state.refreshingFolders.contains(folder.path)
        let refresh = ActionMenuItem(
            title: refreshing
                ? String(localized: "Refreshing\u{2026}")
                : String(localized: "Refresh"),
            symbol: "arrow.clockwise"
        ) { [weak self] in
            self?.actions.onRefreshFolder(folder)
        }
        refresh.isEnabled = !refreshing
        submenu.addItem(refresh)
        submenu.addItem(
            ActionMenuItem(title: String(localized: "Reveal in Finder")) {
                SystemActions.revealInFinder(folder: folder.path)
            }
        )
        submenu.addItem(.separator())
        submenu.addItem(
            ActionMenuItem(title: String(localized: "Remove Folder")) {
                [weak self] in
                self?.actions.onRemoveFolder(folder.path)
            }
        )
        item.submenu = submenu
        return item
    }

    /// The tooltip line saying how a network folder is checked.
    private static func networkLine() -> String {
        coreString(
            bridgeNetworkFolderWatchKey(),
            Int(bridgeNetworkFolderCheckMinutes())
        )
    }

    private static func symbol(
        _ name: String,
        color: NSColor? = nil
    ) -> NSImage? {
        let image = NSImage(
            systemSymbolName: name,
            accessibilityDescription: nil
        )
        guard let color else { return image }
        return image?
            .withSymbolConfiguration(
                NSImage.SymbolConfiguration(paletteColors: [color])
            )
    }
}

/// A menu item that runs a closure when chosen.
@MainActor
final class ActionMenuItem: NSMenuItem {
    private let perform: () -> Void

    init(title: String, symbol: String? = nil, perform: @escaping () -> Void) {
        self.perform = perform
        super.init(title: title, action: #selector(choose), keyEquivalent: "")
        target = self
        if let symbol {
            image = NSImage(
                systemSymbolName: symbol,
                accessibilityDescription: nil
            )
        }
    }

    @available(*, unavailable)
    required init(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc
    private func choose() {
        perform()
    }
}
