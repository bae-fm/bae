import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The import list before any folder is watched and when the list could
    /// not be read.
    static let importListScenes: [ShotScene] = [
        ShotScene(
            id: "mac.import.list.empty",
            size: CGSize(width: 1_200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let uiStore = UiStore()
            uiStore.activeSection = .importing
            let store = ImportStore()
            store.applySummary(
                PreviewData.importQueueSummary(hasWatchedFolders: false)
            )
            let fixture = ImportPreviewFixture(
                store: store,
                itemsByTab: [:],
                tabCounts: BridgeTriageTabCounts(
                    pending: 0,
                    done: 0,
                    skipped: 0
                )
            )
            let images = coverStore()
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: boardLibraryBacking(uiStore: uiStore),
                    playbackStore: boardPlayback(),
                    importScene: fixture,
                    images: images,
                    configStore: PreviewData.makeConfigStore(
                        libraryFullWidth: false,
                        castEnabled: true,
                        discogsUsable: false
                    ),
                    size: CGSize(width: 1_200, height: 760)
                ),
                images: images
            )
        },
        ShotScene(
            id: "mac.import.list.failed",
            size: CGSize(width: 1_200, height: 760)
        ) {
            let uiStore = UiStore()
            uiStore.activeSection = .importing
            let backing = boardLibraryBacking(uiStore: uiStore)
            // The slot reports its failure to a store of its own, so no
            // error alert opens over the window: the board shows the notice
            // alone.
            let slot = ImportListSlot(
                importStore: ImportStore(),
                uiStore: UiStore(),
                selection: ImportSelection(),
                makeHandle: {
                    throw BridgeError.Diagnostic(
                        category: .databaseBusy,
                        detail: LibraryView.unreadableDetail
                    )
                }
            )
            slot.startLoad()
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    playbackStore: boardPlayback(),
                    importListSlot: slot,
                    size: CGSize(width: 1_200, height: 760)
                )
            ) {
                slot.loadFailure != nil
            }
        },
    ]
}
