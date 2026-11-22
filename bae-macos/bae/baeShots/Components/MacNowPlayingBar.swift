import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacNowPlayingBar`: playing, waiting on the cloud, with nothing
    /// played, with no cover, casting to a receiver that takes the volume and
    /// to one that does not, and with the queue open.
    static let macNowPlayingBarScenes: [ShotScene] = {
        let playing = BarLoad(
            held: [BridgeTrackSpan(start: 0, end: 0.6)],
            progress: 102.0 / 303.0
        )
        return [
            nowPlayingBar(
                "mac.component.now-playing-bar.playing",
                title: "Low Tide Hours",
                playing: true,
                load: playing
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.waiting-on-the-cloud",
                title: "Low Tide Hours",
                playing: true,
                load: BarLoad(
                    held: [BridgeTrackSpan(start: 0, end: 0.04)],
                    waiting: true,
                    progress: 5.4 / 303.0
                )
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.nothing-played",
                title: nil,
                playing: false,
                load: BarLoad(held: [], progress: 0, hasPosition: false)
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.no-cover",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(covered: false)
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.casting",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(casting: castReceiver("Living Room", .cast))
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.no-volume",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(
                    casting: castReceiver(
                        "Living Room",
                        .dlna,
                        takesVolume: false
                    )
                )
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.queue-open",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(queueOpen: true)
            ),
        ]
    }()

    /// What the bar's seek bar shows of the track.
    private struct BarLoad {
        var held: [BridgeTrackSpan]
        var waiting = false
        var progress: Double
        var hasPosition = true
    }

    /// What the bar's track and its right edge show beside the seek bar.
    private struct BarShow {
        var covered = true
        var casting: BridgeCastReceiver?
        var queueOpen = false
    }

    private static func nowPlayingBar(
        _ id: String,
        title: String?,
        playing: Bool,
        load: BarLoad,
        show: BarShow = BarShow()
    ) -> ShotScene {
        ShotScene(
            id: id,
            size: CGSize(width: 1100, height: 100)
        ) {
            let images = ImageStore(
                fetchLibraryImageBytes: { try BoardArtwork.data(named: $0.id) }
            )
            return ShotStage(
                NowPlayingBarPreview(
                    trackTitle: title,
                    artistNames: title == nil ? nil : "Mara Quill",
                    cover: title.flatMap { title in
                        show.covered
                            ? BridgeImageRef(
                                id: title,
                                version: "1",
                                imageType: .cover
                            ) : nil
                    },
                    isPlaying: playing,
                    held: load.held,
                    waitsOnStorage: load.waiting,
                    repeatMode: .off,
                    shuffled: false,
                    hasPosition: load.hasPosition,
                    progress: load.progress,
                    casting: show.casting,
                    queueOpen: show.queueOpen
                )
                .environment(images)
                .environment(PreviewData.queueStore(manualCount: 2))
                .environment(Queue.stub())
                .frame(maxHeight: .infinity, alignment: .top),
                images: images
            )
        }
    }
}
