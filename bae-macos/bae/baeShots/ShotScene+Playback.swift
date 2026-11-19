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
            uiStore.setQueuePresented(true)
            let backing = boardLibraryBacking(uiStore: uiStore)
            let playbackStore = boardPlayback(removedFromLibrary: true)
            playbackStore.applyQueueSnapshot(
                BridgeQueueSnapshot(
                    manual: [
                        queueEntry(
                            "Margins",
                            album: "Northbound",
                            durationMs: 295_000
                        ),
                        queueEntry(
                            "Night Ferry",
                            album: "Quiet Engines",
                            durationMs: 276_000
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

    /// One of Mara Quill's tracks in the queue, as the boards list it.
    private static func queueEntry(
        _ title: String,
        album: String,
        durationMs: UInt64
    ) -> BridgeQueueEntry {
        BridgeQueueEntry(
            entryId: "entry-\(title)",
            trackId: "track-\(title)",
            display: BridgeTrackDisplay(
                title: title,
                artistLine: .named("Mara Quill"),
                albumId: "album-\(album)",
                releaseId: "release-\(album)",
                albumTitle: album,
                coverImage: nil
            ),
            durationClock: BridgeDurationClock.length(ms: durationMs)
        )
    }
}
