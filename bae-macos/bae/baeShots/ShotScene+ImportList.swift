import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The import list's boards that need no folders behind them: the main
    /// window on Import, the list that could not be read shown in place of
    /// it, with a failure that can be tried again.
    static let importListScenes: [ShotScene] = [
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
                makeSource: { _ in
                    throw BridgeError.Diagnostic(
                        category: .databaseBusy,
                        detail: LibraryView.unreadableDetail
                    )
                }
            )
            slot.startLoad()
            return ShotStage(
                PreviewScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    needsAttention: false,
                    playbackStore: boardPlayback(),
                    importListSlot: slot,
                    size: CGSize(width: 1_200, height: 760)
                )
            ) {
                slot.loadFailure != nil
            }
        }
    ]
}
