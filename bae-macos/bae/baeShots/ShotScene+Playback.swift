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
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                albumCount: 6,
                covered: true
            )
            let playbackStore = boardPlayback(covered: true)
            let manual = manualQueue
            playbackStore.applyQueueSnapshot(
                BridgeQueueSnapshot(
                    manual: manual,
                    context: BridgePlaybackContext(
                        kind: .release,
                        sourceTitle: "Low Tide Hours"
                    ),
                    shuffled: false,
                    hasNext: true,
                    hasPrevious: false,
                    upNextCount: UInt32(manual.count),
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
                playbackStore: playbackStore,
                images: images,
                size: CGSize(width: 1200, height: 760),
                window: AnyView(
                    MacMainWindowContainer(
                        initialHoveredQueueEntry: manual[1].id
                    )
                    .environment(queue)
                )
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
                albumCount: 6,
                covered: true
            )
            let playbackStore = boardPlayback(removedFromLibrary: true)
            let images = coverStore()
            let manual = manualQueue
            let upcoming = boardUpcoming
            let queue = Queue(upcomingQueries: { _ in
                .fixed(upcoming, answers: UnansweredCount())
            })
            playbackStore.applyQueueSnapshot(
                BridgeQueueSnapshot(
                    manual: manual,
                    context: BridgePlaybackContext(
                        kind: .release,
                        sourceTitle: "Low Tide Hours"
                    ),
                    shuffled: false,
                    hasNext: true,
                    hasPrevious: false,
                    upNextCount: UInt32(manual.count),
                    isEmpty: false
                )
            )
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    playbackStore: playbackStore,
                    images: images,
                    size: CGSize(width: 1200, height: 760),
                    window: AnyView(MacMainWindowContainer().environment(queue))
                ),
                images: images,
                isReady: {
                    backing.session.albums.list?.positionCount == 6
                        && playbackStore.upcomingTotal == upcoming.count
                }
            )
        },
    ]

    private static let manualQueue = [
        queueEntry("Margins", album: "Northbound", duration: 295_000),
        queueEntry("Night Ferry", album: "Quiet Engines", duration: 276_000),
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
            duration: $0.1
        )
    }
}
