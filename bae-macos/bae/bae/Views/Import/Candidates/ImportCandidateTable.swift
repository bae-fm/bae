import AppKit
import BaeKit
import Combine
import SwiftUI

/// The import list's rows as an AppKit table: one row per position of core's
/// flat list, headers included, every row one height, unloaded positions
/// drawn blank until their page lands. Each row is the SwiftUI view the list
/// has always drawn, hosted in a cell.
///
/// The table keeps no selection of its own. Each delivery sets its highlight
/// from the rows core marks selected, and what a person does to the rows —
/// clicks, Command- and Shift-clicks, arrow keys, typing a row's first
/// letters, Select All — goes to core as the change it is.
struct ImportCandidateTable: NSViewRepresentable {
    let list: PaginatedList<BridgeImportListItem>
    let importStore: ImportStore
    /// Holds the person's place in the list across tab switches.
    let listSlot: ImportListSlot
    let actions: ImportListEntryActions
    /// Positions a reveal asks to see, scrolled to the middle of the table.
    let reveals: AnyPublisher<Int, Never>
    /// Whether a reveal is on its way, which does the scrolling until it
    /// lands.
    let revealInProgress: Bool
    /// The person changed the selection.
    let onPersonSelected: () -> Void

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(Importer.self)
    private var importer: Importer?
    @Environment(\.candidateRuntimes)
    private var candidateRuntimes
    @Environment(\.importPercents)
    private var importPercents
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(\.surfaceTone)
    private var tone
    @Environment(\.accentChoice)
    private var accent

    private var rowEnvironment: ImportListRowEnvironment {
        ImportListRowEnvironment(
            imageStore: imageStore,
            importer: importer,
            candidateRuntimes: candidateRuntimes,
            importPercents: importPercents,
            outboxStore: outboxStore,
            appearance: HostedAppearance(
                tone: tone,
                accent: accent
            )
        )
    }

    func makeCoordinator() -> ImportCandidateTableCoordinator {
        ImportCandidateTableCoordinator(
            list: list,
            importStore: importStore,
            listSlot: listSlot,
            actions: actions,
            environment: rowEnvironment,
            onPersonSelected: onPersonSelected
        )
    }

    func makeNSView(context: Context) -> NSScrollView {
        let tableView = ImportCandidateTableView()
        tableView.style = .plain
        tableView.headerView = nil
        tableView.backgroundColor = .clear
        tableView.intercellSpacing = .zero
        tableView.rowHeight = ImportCandidateTableLayout.rowHeight
        tableView.usesAutomaticRowHeights = false
        tableView.allowsMultipleSelection = true
        tableView.allowsEmptySelection = true
        tableView.allowsTypeSelect = true
        tableView.focusRingType = .none
        tableView.setAccessibilityIdentifier("import-list")
        tableView.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        let column = NSTableColumn(
            identifier: NSUserInterfaceItemIdentifier("entry")
        )
        column.resizingMask = .autoresizingMask
        column.isEditable = false
        tableView.addTableColumn(column)

        let scrollView = NSScrollView()
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        scrollView.automaticallyAdjustsContentInsets = false
        context.coordinator.attach(
            tableView,
            in: scrollView,
            reveals: reveals
        )
        return scrollView
    }

    func updateNSView(_: NSScrollView, context: Context) {
        let coordinator = context.coordinator
        coordinator.actions = actions
        coordinator.environment = rowEnvironment
        coordinator.onPersonSelected = onPersonSelected
        // Reading these `@Observable` fields here is what makes SwiftUI
        // update the table when the list delivers or its selection moves.
        let rows = ImportCandidateTableRows(
            count: list.totalCount,
            revision: list.contentRevision
        )
        let summary = importStore.summary
        coordinator.update(
            list: list,
            rows: rows,
            read: ImportCandidateListContentRead(
                revision: rows.revision,
                narrowing: summary.narrowing,
                openedAt: Int(summary.openedAt)
            ),
            shownSelected: importStore.shownSelectedKeys,
            revealing: revealInProgress
        )
    }

    static func dismantleNSView(
        _: NSScrollView,
        coordinator: ImportCandidateTableCoordinator
    ) {
        coordinator.detach()
    }
}

/// Which rows a table shows of its list: how many, and the revision of
/// their content.
struct ImportCandidateTableRows: Equatable {
    let count: Int
    let revision: UInt64
}
