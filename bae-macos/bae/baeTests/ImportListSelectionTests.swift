import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The list's rows selected through its table: each press and key changes
/// the keys the screen keeps, the table highlights the rows they name, and
/// Select All asks core for every row the list shows.
@Suite("Import list selection")
struct ImportListSelectionTests {
    @MainActor
    @Test("a click on a row selects it")
    func candidateCanBeSelected() async throws {
        try await assertCandidateCanBeSelected(grouped: false)
    }

    @MainActor
    @Test("a click on a group member selects it")
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
        let slot = try await Self.selectingSlot(
            items: (grouped ? [Self.groupHeader] : [])
                + [
                    PreviewData.candidateItem(
                        PreviewData.triageRowIdentified,
                        headerKey: grouped ? Self.groupHeader.id : nil
                    )
                ],
            store: store,
            uiStore: uiStore
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            let row = grouped ? 1 : 0
            try ImportListHosting.press(row: row, in: table)

            try await Wait.until(notices: [.selection(of: table)]) {
                table.selectedRowIndexes == [row]
                    && slot.selection.keys == [
                        PreviewData.triageRowIdentified.candidateKey
                    ]
            }
        }
    }

    /// The table highlights the rows whose keys the screen keeps: a click
    /// replaces them, and the highlight follows at once.
    @MainActor
    @Test("the highlight follows the kept keys")
    func theHighlightFollowsTheKeptKeys() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let slot = try await Self.selectingSlot(
            items: [
                PreviewData.candidateItem(PreviewData.triageRowIdentified),
                PreviewData.candidateItem(PreviewData.triageRowUnidentified),
            ],
            store: store,
            uiStore: uiStore,
            selected: [PreviewData.triageRowIdentified.candidateKey]
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            #expect(table.selectedRowIndexes == [0])
            let secondKey = PreviewData.triageRowUnidentified.candidateKey

            try ImportListHosting.press(row: 1, in: table)
            try await Wait.until(notices: [.selection(of: table)]) {
                table.selectedRowIndexes == [1]
                    && slot.selection.keys == [secondKey]
            }
        }
    }

    /// Command-click adds a row and takes one out, keeping the rest.
    @MainActor
    @Test("a Command-click toggles its row in the kept keys")
    func commandClickToggles() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let hidden = Data("/library/hidden".utf8)
        let slot = try await Self.selectingSlot(
            items: [
                PreviewData.candidateItem(PreviewData.triageRowIdentified),
                PreviewData.candidateItem(PreviewData.triageRowUnidentified),
            ],
            store: store,
            uiStore: uiStore,
            selected: [PreviewData.triageRowIdentified.candidateKey, hidden]
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            try ImportListHosting.press(row: 1, in: table, modifiers: .command)

            try await Wait.until(notices: [.selection(of: table)]) {
                table.selectedRowIndexes == [0, 1]
            }
            #expect(
                slot.selection.keys == [
                    PreviewData.triageRowIdentified.candidateKey,
                    PreviewData.triageRowUnidentified.candidateKey,
                    hidden,
                ],
                "a key whose row the list hides stays selected"
            )
        }
    }

    /// A header is never selected: a press on one changes nothing, and the
    /// down arrow from the row above it goes on to the row past it.
    @MainActor
    @Test("headers are skipped by clicks and the arrow keys")
    func headersAreNotSelected() async throws {
        let uiStore = UiStore()
        let store = PreviewData.importTabScene().store
        let header = PreviewData.groupHeaderItem(
            key: BridgeFolderReleaseDecisionKey(
                watchedFolderPath: Data("/library".utf8),
                relativeFolder: Data("Group".utf8)
            ),
            name: "Group",
            entryCount: 1
        )
        let first = PreviewData.triageRowIdentified.candidateKey
        let slot = try await Self.selectingSlot(
            items: [
                PreviewData.candidateItem(PreviewData.triageRowIdentified),
                header,
                PreviewData.candidateItem(
                    PreviewData.triageRowUnidentified,
                    headerKey: header.id
                ),
            ],
            store: store,
            uiStore: uiStore,
            selected: [first]
        )
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore
        ) { table in
            let window = try #require(table.window)
            try ImportListHosting.press(row: 1, in: table)
            #expect(slot.selection.keys == [first])
            #expect(table.selectedRowIndexes == [0])

            #expect(HostedInput.focus(table, in: window))
            try HostedInput.keyDown(.downArrow, in: window)
            try await Wait.until(notices: [.selection(of: table)]) {
                table.selectedRowIndexes == [2]
            }
            #expect(
                slot.selection.keys == [
                    PreviewData.triageRowUnidentified.candidateKey
                ]
            )
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
        let everything: [Data] = [
            PreviewData.triageRowIdentified.candidateKey,
            Data("/library/not-loaded".utf8),
        ]
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                PreviewData.candidateItem(PreviewData.triageRowIdentified)
            ],
            selection: ImportSelection(
                operations: .stub(allKeys: { everything })
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

            try await Wait.until(notices: [.selection(of: table)]) {
                table.selectedRowIndexes == [0]
            }
            #expect(slot.selection.keys == Set(everything))
        }
    }

    /// A slot over `items`, with `selected` kept selected once the list has
    /// loaded.
    @MainActor
    private static func selectingSlot(
        items: [BridgeImportListItem],
        store: ImportStore,
        uiStore: UiStore,
        selected: Set<Data> = []
    ) async throws -> ImportListSlot {
        let source = MutableImportList(items: items)
        let slot = ImportListSlot(
            importStore: store,
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { _ in source.handle }
        )
        slot.startLoad()
        try await Wait.until {
            slot.list?.row(at: items.count - 1) != nil
        }
        slot.selection.select(selected)
        return slot
    }

    /// The header of a group, `Group`, under the library's folder.
    private static let groupHeader = PreviewData.groupHeaderItem(
        key: BridgeFolderReleaseDecisionKey(
            watchedFolderPath: Data("/library".utf8),
            relativeFolder: Data("Group".utf8)
        ),
        name: "Group",
        entryCount: 1
    )
}

/// The import list hosted the way the import view hosts it, for tests that
/// drive its table.
@MainActor
enum ImportListHosting {
    /// The list wired the way the import view wires it, its rows' covers
    /// read from `imageStore`.
    static func list(
        store: ImportStore,
        slot: ImportListSlot,
        uiStore: UiStore,
        imageStore: ImageStore = .stub()
    ) -> some View {
        // The list draws once the queue's summary has landed, as in the app.
        if store.summary == nil {
            store.applySummary(settledSummary)
        }
        return MacImportSidebarContainer(
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
        .environment(imageStore)
    }

    /// A summary of a queue with nothing watched and no narrowing.
    static let settledSummary = BridgeImportQueueSummary(
        hasWatchedFolders: false,
        hasGroups: false,
        narrowing: BridgeImportListNarrowing(
            tab: .pending,
            filterText: "",
            pendingFilter: .all
        )
    )

    /// Host the list, wait for its table to lay out its rows, and hand the
    /// table to `body`.
    static func withList(
        store: ImportStore,
        slot: ImportListSlot,
        uiStore: UiStore,
        imageStore: ImageStore = .stub(),
        size: NSSize = NSSize(width: 400, height: 320),
        _ body: (ImportCandidateTableView) async throws -> Void
    ) async throws {
        try await SnapshotTestSupport.withHostedWindow(
            list(
                store: store,
                slot: slot,
                uiStore: uiStore,
                imageStore: imageStore
            )
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
