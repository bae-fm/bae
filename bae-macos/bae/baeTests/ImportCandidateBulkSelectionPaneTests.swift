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
        let card = await Self.hostCard()
        defer { Self.dismiss(card.window) }
        let pane = Self.hostPane(
            configStore: PreviewData.connectedConfigStore()
        )
        defer { Self.dismiss(pane.window) }
        await SnapshotTestSupport.settle(pane.host)

        #expect(card.size.width == ImportCandidateBulkSelectionCard.width)
        // A storage checkbox is a real AppKit button, so where one lands in the
        // pane, less where it sits inside the card, is the card's own origin.
        let inCard = try #require(Self.leadingCheckbox(in: card.host))
        let inPane = try #require(Self.leadingCheckbox(in: pane.host))
        let origin = CGPoint(
            x: inPane.minX - inCard.minX,
            y: inPane.minY - inCard.minY
        )
        #expect(
            abs(origin.x - (Self.paneSize.width - card.size.width) / 2) <= 1,
            "the card's leading edge is at \(origin.x)"
        )
        #expect(
            abs(origin.y - (Self.paneSize.height - card.size.height) / 2) <= 1,
            "the card's top edge is at \(origin.y)"
        )
    }

    /// The rows are the actions the selection offers, grouped: what gets the
    /// folders in, where their metadata comes from, and what takes them out of
    /// the queue. An action none of the selected folders offers is no row —
    /// here, retrying an identification neither of them failed.
    @Test("the card's rows are the actions the selection offers, grouped")
    func theRowsAreTheSelectionsOffers() {
        let card = Self.card()

        #expect(card.drawnGroups == [.importing, .metadata, .placement])
        // Combine ends the Import group and is no folder's action.
        #expect(card.rows(in: .importing).map(\.action) == [.importReady, nil])
        #expect(
            card.rows(in: .metadata).map(\.action) == [
                .identify, .resetToTags, .clearMetadata,
            ]
        )
        #expect(card.rows(in: .placement).map(\.action) == [.skip])
    }

    /// Each row states how many of the selected folders its action applies to:
    /// one of the two is ready to import, both can be identified or skipped.
    @Test("a row counts the selected folders its action applies to")
    func aRowCountsTheFoldersItAppliesTo() {
        let card = Self.card()

        #expect(card.rows(in: .importing).map(\.count) == [1, nil])
        #expect(card.rows(in: .metadata).map(\.count) == [2, 2, 2])
        #expect(card.rows(in: .placement).map(\.count) == [2])
    }

    /// Combine applies to the selection as a whole rather than folder by
    /// folder, so it is the one row with no count of its own; every other row
    /// counts the folders the selection offers its action for.
    @Test("Combine carries no count where every action row carries one")
    func combineCarriesNoCount() {
        let card = Self.card()
        let selection = Self.selection()

        for row in card.drawnGroups.flatMap(card.rows(in:)) {
            guard let action = row.action else {
                #expect(row.count == nil)
                continue
            }
            #expect(row.count == selection.candidates(for: action).count)
        }
    }

    /// A group with no row is not drawn, and Import is drawn whatever the
    /// selection offers: Combine is its row.
    @Test("only the Import group survives a selection that offers nothing")
    func anEmptyGroupIsNotDrawn() {
        #expect(Self.card(offers: []).drawnGroups == [.importing])
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
        let local = Self.hostPane(configStore: PreviewData.configStore())
        defer { Self.dismiss(local.window) }
        await SnapshotTestSupport.settle(local.host)
        #expect(Self.checkboxes(in: local.host).isEmpty)

        let cloud = Self.hostPane(
            configStore: PreviewData.connectedConfigStore()
        )
        defer { Self.dismiss(cloud.window) }
        await SnapshotTestSupport.settle(cloud.host)
        #expect(Self.checkboxes(in: cloud.host).count == 2)
        let leading = try #require(Self.leadingCheckbox(in: cloud.host))
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
        .importReady, .identify, .retryIdentification, .resetToTags,
        .clearMetadata, .skip, .restore,
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
        let selection = selection()
        return card(
            offers: selection.offers,
            canCombine: selection.canCombine,
            showsStorageChoices: showsStorageChoices
        )
    }

    private static func card(
        offers: [ImportCandidateActionOffer],
        canCombine: Bool = false,
        showsStorageChoices: Bool = false
    ) -> ImportCandidateBulkSelectionCard {
        ImportCandidateBulkSelectionCard(
            selectedCount: selectedKeys.count,
            offers: offers,
            canCombine: canCombine,
            isRunning: false,
            showsStorageChoices: showsStorageChoices,
            storageCloud: .constant(true),
            storagePinned: .constant(true),
            onPerform: { _ in },
            onCombine: {}
        )
    }

    /// The pane over the two selected folders, at a size far larger than the
    /// card — which is what its centering has to answer for.
    private static func hostPane(
        configStore: ConfigStore
    ) -> (window: NSWindow, host: NSHostingView<AnyView>) {
        let uiStore = UiStore()
        uiStore.setFolderCandidateSelection(selectedKeys)
        return SnapshotTestSupport.hostInWindow(
            AnyView(
                ImportCandidateBulkSelectionPane(
                    storageCloud: .constant(true),
                    storagePinned: .constant(true),
                    onPerform: { _ in },
                    onCombine: {}
                )
                .environment(PreviewData.importTabScene().store)
                .environment(uiStore)
                .environment(configStore)
                .background(Theme.background)
                .frame(width: paneSize.width, height: paneSize.height)
            ),
            size: paneSize
        )
    }

    /// The card hosted on its own: exactly the size it asks for, so where a
    /// control lands in that host is where it sits inside the card.
    private struct HostedCard {
        let window: NSWindow
        let host: NSView
        let size: NSSize
    }

    private static func hostCard() async -> HostedCard {
        let size = cardSize(showsStorageChoices: true)
        let hosted = SnapshotTestSupport.hostInWindow(
            card(showsStorageChoices: true),
            size: size
        )
        await SnapshotTestSupport.settle(hosted.host)
        return HostedCard(
            window: hosted.window,
            host: hosted.host,
            size: size
        )
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

    private static func dismiss(_ window: NSWindow) {
        window.contentView = nil
        window.orderOut(nil)
    }
}
