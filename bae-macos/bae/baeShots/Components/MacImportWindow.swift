import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportWindow` with a folder picked and with none, over the
    /// preview's list of two rows.
    static let macImportWindowScenes: [ShotScene] = [
        component(
            "mac.component.import-window.folder-picked",
            width: 1_000,
            height: 420,
            images: PreviewData.artImageStore()
        ) {
            try importWindow(openingLowTideHours: true)
        },
        component(
            "mac.component.import-window.no-selection",
            width: 1_000,
            height: 420,
            images: PreviewData.artImageStore()
        ) {
            try importWindow(openingLowTideHours: false)
        },
    ]

    /// The Import window with its two Found rows and no work running.
    @MainActor
    private static func importWindow(
        openingLowTideHours: Bool
    ) throws -> some View {
        let cover = try PreviewData.lowTideHoursCover()
        let uiStore = UiStore()
        uiStore.activeSection = .importing
        uiStore.setImportCandidateTab(.pending)
        let store = ImportStore()
        store.applySummary(
            PreviewData.importQueueSummary(
                hasWatchedFolders: true,
                hasGroups: true
            )
        )
        let selection =
            if openingLowTideHours {
                PreviewData.selectBoardCandidate(
                    PreviewData.lowTideHoursCandidate(cover: cover),
                    in: store
                )
            }
            else {
                ImportSelection()
            }
        let fixture = ImportPreviewFixture(
            store: store,
            itemsByTab: [
                .pending: try previewRows(lowTideHoursCover: cover.file)
            ],
            tabCounts: columnCounts
        )
        let slot = fixture.slot(uiStore: uiStore, selection: selection)
        return MacImportWindow(endEditing: {})
            .environment(uiStore)
            .importSectionEnvironment(
                store: fixture.store,
                slot: slot,
                runtimes: [:]
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
            .environment(PreviewData.northboundUploads())
            .mainWindowPreviewEnvironment()
            .frame(width: 1_000, height: 420)
    }

    /// The preview's list: Low Tide Hours read from its release, which
    /// needs nothing from the person, and Copper Sky, not found.
    private static func previewRows(lowTideHoursCover: BridgeFileVersion) throws
        -> [BridgeImportListItem]
    {
        let folder = PreviewData.copperSky
        let copperSkyCover = try PreviewData.notFoundCover(folder)
        return [
            PreviewData.candidateItem(
                PreviewData.boardRow(
                    .lowTideHours,
                    summary: BridgeAlbumLine.titleAndArtists(
                        title: "Low Tide Hours",
                        artistLine: .named("Mara Quill")
                    ),
                    record: "board-low-tide-hours",
                    cover: .local(file: lowTideHoursCover)
                )
            ),
            PreviewData.candidateItem(
                PreviewData.notFoundBoardRow(folder, cover: copperSkyCover.file)
            ),
        ]
    }
}
