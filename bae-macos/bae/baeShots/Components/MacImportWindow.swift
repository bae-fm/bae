import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportWindow` with a folder picked and with none. With none, the
    /// list holds the preview's two rows.
    static let macImportWindowScenes: [ShotScene] = [
        component(
            "mac.component.import-window.folder-picked",
            width: 1_000,
            height: 420,
            images: PreviewData.artImageStore()
        ) {
            importWindow(.found(northbound: .identified, opening: .copperSky))
        },
        component(
            "mac.component.import-window.no-selection",
            width: 1_000,
            height: 420,
            images: PreviewData.artImageStore()
        ) {
            importWindow(
                .found(northbound: .identified, opening: nil),
                items: previewRows()
            )
        },
    ]

    /// The Import window's content as `board` draws it, with no work running
    /// and the list holding two rows, at the preview's 1000 by 420 points.
    @MainActor
    private static func importWindow(
        _ board: PreviewData.ImportBoard,
        items: [BridgeImportListItem] = PreviewData.lookupItems()
    ) -> some View {
        let uiStore = UiStore()
        uiStore.activeSection = .importing
        uiStore.setImportCandidateTab(board.tab)
        let scene = PreviewData.importBoardScene(board)
        scene.fixture.store.identificationProgress = nil
        scene.fixture.store.importsInFlight = 0
        let fixture = ImportPreviewFixture(
            store: scene.fixture.store,
            itemsByTab: [board.tab: items],
            tabCounts: columnCounts
        )
        let slot = fixture.slot(uiStore: uiStore, selection: scene.selection)
        return MacImportWindow(endEditing: {})
            .environment(uiStore)
            .importSectionEnvironment(
                store: fixture.store,
                slot: slot,
                runtimes: scene.runtimes
            )
            .environment(SettingsNavigation())
            .environment(PreviewAudio.stub())
            .environment(Playback.stub())
            .environment(Library.stub())
            .environment(Importer.stub())
            .environment(PreviewData.releaseEditor())
            .environment(
                PreviewData.configStore(storage: PreviewData.previewStorage)
            )
            .environment(scene.outbox)
            .environment(\.importPercents, scene.importPercents)
            .mainWindowPreviewEnvironment()
            .frame(width: 1_000, height: 420)
    }

    /// The preview's list: Low Tide Hours read from its release, which
    /// needs nothing from the person, and Copper Sky, not found.
    private static func previewRows() -> [BridgeImportListItem] {
        [
            PreviewData.candidateItem(
                PreviewData.boardRow(
                    .lowTideHours,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Low Tide Hours",
                        albumArtists: .named("Mara Quill")
                    ),
                    record: "board-low-tide-hours"
                )
            ),
            PreviewData.lookupItems()[1],
        ]
    }
}
