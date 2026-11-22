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
            try importWindow(
                opening: PreviewData.lowTideHoursCandidate(),
                items: try previewRows()
            )
        },
        component(
            "mac.component.import-window.no-selection",
            width: 1_000,
            height: 420,
            images: PreviewData.artImageStore()
        ) {
            try importWindow(
                opening: nil,
                items: try previewRows()
            )
        },
    ]

    /// The Import window with no work running
    /// and the list holding `items`, at the preview's 1000 by 420 points.
    @MainActor
    private static func importWindow(
        opening candidate: Candidate?,
        items: [BridgeImportListItem]
    ) -> some View {
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
            candidate.map { PreviewData.selectBoardCandidate($0, in: store) }
            ?? ImportSelection()
        let fixture = ImportPreviewFixture(
            store: store,
            itemsByTab: [.pending: items],
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
    private static func previewRows() throws -> [BridgeImportListItem] {
        [
            PreviewData.candidateItem(
                try PreviewData.boardRow(
                    .lowTideHours,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Low Tide Hours",
                        albumArtists: .named("Mara Quill")
                    ),
                    record: "board-low-tide-hours"
                )
            ),
            try PreviewData.lookupItems()[1],
        ]
    }
}
