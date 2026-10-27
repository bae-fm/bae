import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The list's rows selected through its table: each press and key reaches
/// core as the selection change it is, the table highlights what core holds,
/// a click is never undone by a list read from before it, and Select All
/// asks core for every row the list shows.
@Suite("Import list selection")
struct ImportListSelectionTests {
    @MainActor
    @Test("a click on a row reaches core as the selection")
    func candidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(grouped: false)
    }

    @MainActor
    @Test("a click on a group member reaches core as the selection")
    func groupedCandidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(grouped: true)
    }

    /// A click on the release, alone or as the member of a group listed
    /// under its header.
    @MainActor
    private func assertCandidateCanBeSelected(grouped: Bool) async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        let store = PreviewData.importTabScene().store
        let changes = CallLog<BridgeSelectionChange>()
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: (grouped ? [Self.groupHeader] : [])
                + [
                    PreviewData.candidateItem(
                        PreviewData.triageRowIdentified,
                        headerPosition: grouped ? 0 : nil
                    )
                ],
            selection: ImportSelection(
                operations: .stub(change: { change in
                    changes.record(change)
                    return 1
                })
            )
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            let row = grouped ? 1 : 0
            try ImportListHosting.press(row: row, in: table)

            try await Wait.until { !changes.all.isEmpty }
            #expect(
                changes.all == [
                    .replace(keys: [
                        PreviewData.triageRowIdentified.candidateKey
                    ])
                ]
            )
            #expect(table.selectedRowIndexes == [row])
        }
    }

    /// A click shows the clicked row selected at once. Core's list catches up
    /// only after the click's write lands, and a list read taken before that —
    /// still selecting the row the click replaced — must not put that row back.
    @MainActor
    @Test(
        "a click stays selected, and a list read from before it can't undo it"
    )
    func aClickIsNotUndoneByAnOlderListRead() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let first = Self.row(PreviewData.triageRowIdentified, selected: true)
        let second = Self.row(
            PreviewData.triageRowUnidentified,
            selected: false
        )
        let written = AsyncStream<Void>.makeStream()
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [first, second],
            selection: ImportSelection(
                operations: .stub(change: { _ in
                    for await _ in written.stream { break }
                    return 2
                })
            )
        )
        store.applySelectionRevision(1)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            #expect(table.selectedRowIndexes == [0])
            let secondKey = PreviewData.triageRowUnidentified.candidateKey

            try ImportListHosting.press(row: 1, in: table)
            try await Wait.until { table.selectedRowIndexes == [1] }

            // A list read from before the click's write: the first row still
            // selected, at the revision the click started from.
            store.ingest([first, second])
            store.applySelectionRevision(1)
            try await Wait.untilSteady { table.selectedRowIndexes }
            #expect(table.selectedRowIndexes == [1])

            // The write lands, and the list read that reflects it.
            written.continuation.yield(())
            store.ingest([
                Self.row(PreviewData.triageRowIdentified, selected: false),
                Self.row(PreviewData.triageRowUnidentified, selected: true),
            ])
            store.applySelectionRevision(2)
            try await Wait.untilSteady { table.selectedRowIndexes }
            #expect(table.selectedRowIndexes == [1])
            #expect(store.shownSelectedKeys == [secondKey])
        }
    }

    /// Command-click adds a row and takes one out, keeping the rest.
    @MainActor
    @Test("a Command-click toggles its row in core's selection")
    func commandClickToggles() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let changes = CallLog<BridgeSelectionChange>()
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                Self.row(PreviewData.triageRowIdentified, selected: true),
                Self.row(PreviewData.triageRowUnidentified, selected: false),
            ],
            selection: ImportSelection(
                operations: .stub(change: { change in
                    changes.record(change)
                    return 1
                })
            )
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            try ImportListHosting.press(row: 1, in: table, modifiers: .command)

            try await Wait.until { !changes.all.isEmpty }
            #expect(
                changes.all == [
                    .toggle(
                        add: [PreviewData.triageRowUnidentified.candidateKey],
                        remove: []
                    )
                ]
            )
            try await Wait.until { table.selectedRowIndexes == [0, 1] }
        }
    }

    /// A header is never selected: a press on one asks nothing of core, and
    /// the down arrow from the row above it goes on to the row past it.
    @MainActor
    @Test("headers are skipped by clicks and the arrow keys")
    func headersAreNotSelected() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let changes = CallLog<BridgeSelectionChange>()
        let header = PreviewData.groupHeaderItem(
            key: BridgeFolderReleaseDecisionKey(
                watchedFolderPath: "/library",
                relativeFolderPath: "Group"
            ),
            name: "Group",
            entryCount: 1
        )
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                Self.row(PreviewData.triageRowIdentified, selected: true),
                header,
                PreviewData.candidateItem(
                    PreviewData.triageRowUnidentified,
                    headerPosition: 1
                ),
            ],
            selection: ImportSelection(
                operations: .stub(change: { change in
                    changes.record(change)
                    return 1
                })
            )
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            let window = try #require(table.window)
            try ImportListHosting.press(row: 1, in: table)
            try await Wait.untilSteady { table.selectedRowIndexes }
            #expect(changes.all.isEmpty)
            #expect(table.selectedRowIndexes == [0])

            #expect(HostedInput.focus(table, in: window))
            try HostedInput.keyDown(.downArrow, in: window)
            try await Wait.until { !changes.all.isEmpty }
            #expect(
                changes.all == [
                    .replace(keys: [
                        PreviewData.triageRowUnidentified.candidateKey
                    ])
                ]
            )
            try await Wait.until { table.selectedRowIndexes == [2] }
        }
    }

    /// Edit ▸ Select All and Command-A go up the responder chain to the
    /// table, which asks core for every row the list's view shows rather
    /// than selecting the rows it has loaded.
    @MainActor
    @Test("Select All asks core for every shown row")
    func selectAllAsksCore() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let selectAlls = CallLog<Void>()
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                PreviewData.candidateItem(PreviewData.triageRowIdentified)
            ],
            selection: ImportSelection(
                operations: .stub(selectAll: { selectAlls.record(()) })
            )
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            let window = try #require(table.window)
            #expect(HostedInput.focus(table, in: window))
            let responder = try #require(window.firstResponder)
            #expect(
                responder.tryToPerform(
                    #selector(NSResponder.selectAll(_:)),
                    with: nil
                )
            )

            try await Wait.until { !selectAlls.all.isEmpty }
            #expect(selectAlls.all.count == 1)
            #expect(table.selectedRowIndexes.isEmpty)
        }
    }

    private static func row(
        _ row: BridgeTriageRow,
        selected: Bool
    ) -> BridgeImportListItem {
        var row = row
        row.selected = selected
        return PreviewData.candidateItem(row)
    }

    /// The header of a group, `Group`, under the library's folder.
    private static let groupHeader = PreviewData.groupHeaderItem(
        key: BridgeFolderReleaseDecisionKey(
            watchedFolderPath: "/library",
            relativeFolderPath: "Group"
        ),
        name: "Group",
        entryCount: 1
    )
}

/// The import list hosted the way the import view hosts it, for tests that
/// drive its table.
@MainActor
enum ImportListHosting {
    /// The list wired the way the import view wires it.
    static func list(
        store: ImportStore,
        slot: ImportListSlot,
        uiStore: UiStore
    ) -> some View {
        ImportCandidateListContent(
            importStore: store,
            listSlot: slot,
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onShowInLibrary: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(OutboxStore(uploadsPaused: false))
        .environment(uiStore)
        .environment(ImportSelection())
        .environment(ImageStore.stub())
    }

    /// Host the list, wait for its table to lay out its rows, and hand the
    /// table to `body`.
    static func withList(
        store: ImportStore,
        slot: ImportListSlot,
        uiStore: UiStore,
        size: NSSize = NSSize(width: 400, height: 320),
        _ body: (ImportCandidateTableView) async throws -> Void
    ) async throws {
        try await SnapshotTestSupport.withHostedWindow(
            list(store: store, slot: slot, uiStore: uiStore)
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            try await body(try table(in: host))
        }
    }

    /// A press on `row`, through the table's own mouse handling.
    static func press(
        row: Int,
        in table: NSTableView,
        modifiers: NSEvent.ModifierFlags = []
    ) throws {
        let rect = table.rect(ofRow: row)
        try HostedInput.track(
            table,
            at: table.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil),
            modifiers: modifiers
        )
    }

    /// The list's table.
    static func table(in host: NSView) throws -> ImportCandidateTableView {
        try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? ImportCandidateTableView }
                .first
        )
    }
}
