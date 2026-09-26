import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The card a selection of several folders is acted on through: what it lists,
/// what each row says it applies to, and where the card sits in the pane.
@MainActor
@Suite("The import bulk-selection pane")
struct ImportCandidateBulkSelectionPaneTests {
    private static let paneSize = NSSize(width: 900, height: 700)

    /// The card keeps its own width and sits at the pane's center on both
    /// axes, so a pane far wider than the card doesn't strand it in a corner.
    @Test("the card is centered in the pane at its own width")
    func theCardIsCenteredInThePane() async throws {
        // A storage checkbox is a real AppKit button, so where one lands in the
        // pane, less where it sits inside the card, is the card's own origin.
        let cardSize = Self.cardSize(showsStorageChoices: true)
        let inCard = try await SnapshotTestSupport.withHostedWindow(
            Self.card(showsStorageChoices: true),
            size: cardSize
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return try #require(Self.leadingCheckbox(in: host))
        }
        let inPane = try await Self.withHostedPane(
            configStore: PreviewData.connectedConfigStore()
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return try #require(Self.leadingCheckbox(in: host))
        }

        #expect(cardSize.width == ImportCandidateBulkSelectionCard.width)
        let origin = CGPoint(
            x: inPane.minX - inCard.minX,
            y: inPane.minY - inCard.minY
        )
        #expect(
            abs(origin.x - (Self.paneSize.width - cardSize.width) / 2) <= 1,
            "the card's leading edge is at \(origin.x)"
        )
        #expect(
            abs(origin.y - (Self.paneSize.height - cardSize.height) / 2) <= 1,
            "the card's top edge is at \(origin.y)"
        )
    }

    /// The rows are the actions the selection offers, grouped: what gets the
    /// folders in, where their metadata comes from, what takes them out of
    /// the queue, and where they are. An action none of the selected folders
    /// offers is no row — here, retrying an identification neither of them
    /// failed.
    @Test("the card's rows are the actions the selection offers, grouped")
    func theRowsAreTheSelectionsOffers() {
        let card = Self.card()

        #expect(
            card.drawnGroups == [.importing, .metadata, .placement, .folder]
        )
        #expect(
            card.rows(in: .importing).map(\.action) == [.importReady, .combine]
        )
        #expect(
            card.rows(in: .metadata).map(\.action) == [
                .identify, .resetToFileMetadata, .clearMetadata,
            ]
        )
        #expect(card.rows(in: .placement).map(\.action) == [.skip])
        #expect(card.rows(in: .folder).map(\.action) == [.revealFolder])
    }

    /// Each row states how many of the selected folders its action applies to:
    /// one of the two is ready to import, both can be identified or skipped.
    @Test("a row counts the selected folders its action applies to")
    func aRowCountsTheFoldersItAppliesTo() {
        let card = Self.card()

        #expect(card.rows(in: .importing).map(\.count) == [1, nil])
        #expect(card.rows(in: .metadata).map(\.count) == [2, 2, 2])
        #expect(card.rows(in: .placement).map(\.count) == [2])
        #expect(card.rows(in: .folder).map(\.count) == [2])
    }

    /// Combine applies to the selection as a whole rather than folder by
    /// folder, so it is the one row with no count of its own; every other row
    /// counts the folders the selection offers its action for.
    @Test("Combine carries no count where every action row carries one")
    func combineCarriesNoCount() {
        let card = Self.card()
        let selection = Self.selection()

        for row in card.drawnGroups.flatMap(card.rows(in:)) {
            guard row.action != .combine else {
                #expect(row.count == nil)
                continue
            }
            #expect(row.count == selection.candidates(for: row.action).count)
        }
    }

    /// A group with no row is not drawn.
    @Test("a selection that offers nothing draws no group")
    func anEmptyGroupIsNotDrawn() {
        #expect(Self.card(offers: []).drawnGroups.isEmpty)
    }

    /// The number of folders belongs beside an action's name, not inside it:
    /// the card draws it as the row's own pill, and a surface with nowhere to
    /// put it separately asks for the name and the number together.
    @Test("an action's name carries no count of its own")
    func anActionsNameCarriesNoCount() {
        for action in Self.everyAction {
            #expect(!action.label.contains { $0.isNumber })
            #expect(action.label(count: 7).contains(action.label))
            #expect(action.label(count: 7).contains("7"))
        }
    }

    /// Where imported files go is a question only for a library with a cloud
    /// home, and it belongs to the import row: the choices sit inside the card,
    /// lined up with the rows' names.
    @Test("the storage choices are drawn only for a library with a cloud home")
    func theStorageChoicesFollowTheCloudHome() async throws {
        try await Self.withHostedPane(
            configStore: PreviewData.configStore()
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            #expect(Self.checkboxes(in: host).isEmpty)
        }

        let leading = try await Self.withHostedPane(
            configStore: PreviewData.connectedConfigStore()
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            #expect(Self.checkboxes(in: host).count == 2)
            return try #require(Self.leadingCheckbox(in: host))
        }
        let name =
            (Self.paneSize.width - ImportCandidateBulkSelectionCard.width) / 2
            + ImportCandidateBulkSelectionCard.padding
            + ImportBulkActionRowMetrics.labelInset
        #expect(
            abs(leading.minX - name) <= 1,
            "the choices start at \(leading.minX), the rows' names at \(name)"
        )
        #expect(
            Self.cardSize(showsStorageChoices: true).height
                > Self.cardSize(showsStorageChoices: false).height
        )
    }

    // MARK: - Staging

    private static let everyAction: [BridgeCandidateAction] = [
        .importReady, .identify, .cancelIdentification, .cancelImport,
        .retryIdentification, .resetToFileMetadata, .clearMetadata, .combine,
        .separate, .skip, .restore, .revealFolder,
    ]

    /// Two selected folders: one ready to import, one whose signals disagree.
    /// A row that applies to one of them is drawn beside rows that apply to
    /// both, which is what the card is for.
    private static let selectedKeys: Set<String> = [
        PreviewData.importTabCandidate.key,
        PreviewData.importTabDisagreementCandidate.key,
    ]

    /// What the two selected folders offer, read the way the pane reads it.
    private static func selection() -> ImportCandidateSelection {
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection(selectedKeys)
        return ImportCandidateSelection(
            importStore: PreviewData.importTabScene().store,
            uiStore: uiStore
        )
    }

    private static func card(
        showsStorageChoices: Bool = false
    ) -> ImportCandidateBulkSelectionCard {
        card(
            offers: selection().offers,
            showsStorageChoices: showsStorageChoices
        )
    }

    private static func card(
        offers: [ImportCandidateActionOffer],
        showsStorageChoices: Bool = false
    ) -> ImportCandidateBulkSelectionCard {
        ImportCandidateBulkSelectionCard(
            selectedCount: selectedKeys.count,
            offers: offers,
            isRunning: false,
            showsStorageChoices: showsStorageChoices,
            storageCloud: .constant(true),
            storagePinned: .constant(true),
            onPerform: { _ in }
        )
    }

    /// The pane over the two selected folders, at a size far larger than the
    /// card — which is what its centering has to answer for.
    private static func withHostedPane<Value>(
        configStore: ConfigStore,
        _ body: (NSWindow, NSHostingView<AnyView>) async throws -> Value
    ) async throws -> Value {
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection(selectedKeys)
        return try await SnapshotTestSupport.withHostedWindow(
            AnyView(
                ImportCandidateBulkSelectionPane(
                    storageCloud: .constant(true),
                    storagePinned: .constant(true),
                    onPerform: { _ in }
                )
                .environment(PreviewData.importTabScene().store)
                .environment(uiStore)
                .environment(configStore)
                .background(Theme.background)
                .frame(width: paneSize.width, height: paneSize.height)
            ),
            size: paneSize
        ) {
            try await body($0, $1)
        }
    }

    private static func cardSize(showsStorageChoices: Bool) -> NSSize {
        NSHostingView(rootView: card(showsStorageChoices: showsStorageChoices))
            .fittingSize
    }

    /// SwiftUI's checkbox is an AppKit button, so the storage choices are the
    /// buttons the hosted tree holds.
    private static func checkboxes(in host: NSView) -> [NSButton] {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSButton }
    }

    private static func leadingCheckbox(in host: NSView) -> CGRect? {
        checkboxes(in: host)
            .map { $0.convert($0.bounds, to: host) }
            .min { $0.minX < $1.minX }
    }

}
