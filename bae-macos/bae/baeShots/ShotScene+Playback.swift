import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The playback boards drawn over the library window: the queue open
    /// while the playing track has been deleted from the library.
    static let playbackScenes: [ShotScene] = [
        ShotScene(
            id: "mac.playback.queue.track-removed",
            size: CGSize(width: 1200, height: 760)
        ) {
            let uiStore = UiStore()
            uiStore.setSidePanePresented(true)
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                albumCount: 6
            )
            let playbackStore = boardPlayback(removedFromLibrary: true)
            playbackStore.applyQueueSnapshot(
                BridgeQueueSnapshot(
                    manual: [
                        queueEntry(
                            "Margins",
                            album: "Northbound",
                            duration: 295_000,
                            covered: false
                        ),
                        queueEntry(
                            "Night Ferry",
                            album: "Quiet Engines",
                            duration: 276_000,
                            covered: false
                        ),
                    ],
                    context: nil,
                    shuffled: false,
                    hasNext: true,
                    hasPrevious: false,
                    upNextCount: 2,
                    isEmpty: false
                )
            )
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    needsAttention: false,
                    playbackStore: playbackStore,
                    size: CGSize(width: 1200, height: 760)
                )
            )
        }
    ]
}
