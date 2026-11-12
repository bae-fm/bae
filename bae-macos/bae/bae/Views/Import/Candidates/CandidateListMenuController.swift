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
    /// Whether the queue has folder groups; without any, Expand all and
    /// Collapse all are disabled.
    let hasGroups: Bool
    let sortOrder: BridgeImportListOrder
    /// The entry Found's rows are narrowed to.
    let pendingFilter: BridgePendingFilter
    /// Every entry of Found's filter, in core's order, with how many rows
    /// each holds.
    let pendingFilterEntries: [BridgePendingFilterEntry]
    /// Whether the tab on show is Found, the one tab the filter applies to.
    let pendingFilterApplies: Bool

    /// Whether any root's last scan failed or left it not watched, which
    /// also marks the trigger.
    var hasScanProblem: Bool {
        scanStatuses.values.contains { status in
            switch status {
            case .failed, .notWatched: true
            case .scanning, .complete: false
            }
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
/// Drawn by bae's menu, built from the state as it stands each time it
/// opens. While it is open, a change to the filter's counts or to what can be
/// chosen is shown in its lines where they stand, so the counts on screen are
/// the list's latest and the menu stays open under the pointer. Everything
/// else it shows is taken up the next time it opens.
@MainActor
final class CandidateListMenuController {
    private(set) var state: CandidateListMenuState
    var actions: CandidateListMenuActions
    private var handle: MacMenuHandle?

    init(state: CandidateListMenuState, actions: CandidateListMenuActions) {
        self.state = state
        self.actions = actions
    }

    /// Show `state`: an open menu takes the filter's entries at once, the
    /// rest of it when the menu next opens.
    func show(_ state: CandidateListMenuState) {
        let before = filterLines(of: self.state)
        self.state = state
        guard let handle, handle.isOpen else { return }
        if filterLines(of: state) != before {
            handle.update(entries)
        }
    }

    /// Opens the menu under `view`.
    func present(from view: NSView, appearance: HostedAppearance) {
        guard let window = view.window else { return }
        handle = MacMenus.present(
            entries,
            over: window,
            at: .below(view),
            appearance: appearance
        )
    }

    /// What Found's filter shows: each entry's words, whether it can be
    /// chosen and whether it is the chosen one.
    private func filterLines(
        of state: CandidateListMenuState
    ) -> [String] {
        state.pendingFilterEntries.map { entry in
            [
                entry.filter.label(count: entry.count),
                String(state.pendingFilterApplies && entry.selectable),
                String(entry.filter == state.pendingFilter),
            ]
            .joined(separator: "|")
        }
    }

    // MARK: Building

    /// The menu's lines as the state stands now.
    var entries: [MacMenuEntry] {
        var lines: [MacMenuEntry] = [.header(String(localized: "Sort"))]
        for (title, order) in Self.sortOrders {
            lines.append(
                .item(
                    MacMenuItem(
                        verbatim: title,
                        checked: order == state.sortOrder
                    ) { [weak self] in
                        self?.actions.onSetSortOrder(order)
                    }
                )
            )
        }
        lines.append(.divider)
        lines.append(.header(String(localized: "Filter")))
        lines.append(contentsOf: state.pendingFilterEntries.map(filterLine))
        lines.append(.divider)
        lines.append(.header(String(localized: "Folders")))
        lines.append(
            .item(
                MacMenuItem(verbatim: String(localized: "Add a folder\u{2026}"))
                { [weak self] in
                    self?.actions.onAddFolder()
                }
            )
        )
        lines.append(contentsOf: state.watchedFolders.map(folderLine))
        lines.append(.divider)
        lines.append(.header(String(localized: "Groups")))
        for (title, expanded) in [
            (String(localized: "Expand all"), true),
            (String(localized: "Collapse all"), false),
        ] {
            lines.append(
                .item(
                    MacMenuItem(verbatim: title, disabled: !state.hasGroups) {
                        [weak self] in
                        self?.actions.onSetAllGroupsExpanded(expanded)
                    }
                )
            )
        }
        return lines
    }

    private static let sortOrders: [(String, BridgeImportListOrder)] = [
        (String(localized: "Newest first"), .newestFirst),
        (String(localized: "Oldest first"), .oldestFirst),
        (String(localized: "Folder name (A–Z)"), .pathAscending),
        (String(localized: "Folder name (Z–A)"), .pathDescending),
    ]

    /// One entry of Found's filter, checked when it is the chosen one: an
    /// entry holding no row cannot be chosen, one already chosen stays
    /// checked, none can be chosen off Found, and choosing the chosen entry
    /// again changes nothing.
    private func filterLine(_ entry: BridgePendingFilterEntry) -> MacMenuEntry {
        let filter = entry.filter
        return .item(
            MacMenuItem(
                verbatim: filter.label(count: entry.count),
                checked: filter == state.pendingFilter,
                disabled: !(state.pendingFilterApplies && entry.selectable)
            ) { [weak self] in
                guard let self, filter != state.pendingFilter else { return }
                actions.onSetPendingFilter(filter)
            }
        )
    }

    /// One root's line: its scan state is the glyph, and a failed scan's
    /// error or a folder's reason it is not watched is the tooltip.
    private func folderLine(_ folder: BridgeWatchedFolder) -> MacMenuEntry {
        let onNetwork = state.networkFolders.contains(folder.path)
        let networkLine = onNetwork ? Self.networkLine() : nil
        let accessory: MacMenuAccessory?
        switch state.scanStatuses[folder.path] {
        case .scanning:
            accessory = MacMenuAccessory(
                symbol: "progress.indicator",
                tone: .secondary,
                help: networkLine
            )
        case .failed(let detail), .notWatched(let detail):
            accessory = MacMenuAccessory(
                symbol: "exclamationmark.circle.fill",
                tone: .danger,
                help: [detail, networkLine].compactMap { $0 }
                    .joined(separator: "\n")
            )
        case .complete, nil:
            accessory =
                onNetwork
                ? MacMenuAccessory(
                    symbol: "network",
                    tone: .secondary,
                    help: networkLine
                ) : nil
        }
        let refreshing = state.refreshingFolders.contains(folder.path)
        let submenu = MacMenuSubmenu(
            verbatim: folder.name,
            accessory: accessory
        ) {
            MacMenuItem(
                verbatim: refreshing
                    ? String(localized: "Refreshing\u{2026}")
                    : String(localized: "Refresh"),
                disabled: refreshing
            ) { [weak self] in
                self?.actions.onRefreshFolder(folder)
            }
            MacMenuItem("Reveal in Finder") {
                SystemActions.revealInFinder(folder: folder.path)
            }
            MacMenuDivider()
            MacMenuItem("Remove folder") { [weak self] in
                self?.actions.onRemoveFolder(folder.path)
            }
        }
        return submenu.entries[0]
    }

    /// The tooltip line saying how a network folder is checked.
    private static func networkLine() -> String {
        localizedCoreString(
            bridgeNetworkFolderWatchKey(),
            Int(bridgeNetworkFolderCheckMinutes())
        )
    }
}
