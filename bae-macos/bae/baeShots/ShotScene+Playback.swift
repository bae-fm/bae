import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The queue open over the library window, with a track playing and
    /// after the playing track was deleted from the library.
    static let playbackScenes: [ShotScene] = [
        ShotScene(
            id: "mac.playback.queue",
            size: CGSize(width: 1200, height: 760)
        ) {
            let uiStore = UiStore()
            uiStore.setSidePanePresented(true)
            let backing = boardLibraryBacking(uiStore: uiStore, albumCount: 6)
            let playbackStore = boardPlayback()
            playbackStore.applyQueueSnapshot(
                BridgeQueueSnapshot(
                    manual: manualQueue,
                    context: BridgePlaybackContext(
                        kind: .release,
                        sourceTitle: "Low Tide Hours"
                    ),
                    shuffled: false,
                    hasNext: true,
                    hasPrevious: false,
                    upNextCount: UInt32(manualQueue.count),
                    isEmpty: false
                )
            )
            let images = coverStore()
            let answers = UnansweredCount()
            let upcoming = boardUpcoming
            let queue = Queue(upcomingQueries: { _ in
                .fixed(upcoming, answers: answers)
            })
            let window = ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: backing,
                needsAttention: false,
                playbackStore: playbackStore,
                images: images,
                size: CGSize(width: 1200, height: 760),
                window: AnyView(MacMainWindowContainer().environment(queue))
            )
            return ShotStage(window, images: images) {
                backing.session.albums.list?.delivered != nil
                    && playbackStore.upcomingTotal == upcoming.count
                    && playbackStore.upcomingItem(at: upcoming.count - 1) != nil
                    && answers.value == 0
            }
        },
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
                    manual: manualQueue,
                    context: nil,
                    shuffled: false,
                    hasNext: true,
                    hasPrevious: false,
                    upNextCount: UInt32(manualQueue.count),
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
        },
    ]

    private static let manualQueue = [
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
    ]

    private static let boardUpcoming = [
        ("Tin Roof", UInt64(341_000)),
        ("Margins", 295_000),
        ("Weather Report Card", 276_000),
        ("Old Dog", 177_000),
        ("Night Ferry", 276_000),
    ]
    .map {
        queueEntry(
            $0.0,
            album: "Low Tide Hours",
            duration: $0.1,
            covered: false
        )
    }
}
