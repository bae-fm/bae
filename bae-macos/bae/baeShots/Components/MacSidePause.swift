import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSidePause`, the card that hangs from the top of the window: the
    /// side's end with the countdown running, the side's end without one, and
    /// the end of a disc.
    static let macSidePauseScenes: [ShotScene] = [
        sidePause(
            "mac.component.side-pause.counting-down",
            ended: .sided(sideLetter: "A"),
            countdownKey: "core.playback.pause.side_ended.countdown"
        ),
        sidePause(
            "mac.component.side-pause.paused",
            ended: .sided(sideLetter: "A"),
            countdownKey: nil
        ),
        sidePause(
            "mac.component.side-pause.disc",
            ended: .disc(disc: 2),
            countdownKey: "core.playback.pause.disc_ended.countdown"
        ),
    ]

    private static func sidePause(
        _ id: String,
        ended: BridgeSideHeading,
        countdownKey: String?
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 560, height: 340)) {
            let store = PlaybackStore()
            let prompt = BridgeSidePausePrompt(
                id: "side-pause",
                ended: ended,
                countdown: countdownKey.map {
                    BridgeCountdown(
                        key: $0,
                        until: Date().addingTimeInterval(11.95)
                    )
                }
            )
            store.apply(
                .paused(
                    track: BridgeNowPlayingTrack(
                        track: BridgePlayingTrack(
                            trackId: "t-harbor-lights",
                            durationMs: nil
                        ),
                        display: .library(
                            display: BridgeTrackDisplay(
                                title: "Harbor Lights",
                                artistLine: .named("Mara Quill"),
                                albumId: "a-01",
                                releaseId: "r-01",
                                albumTitle: "Low Tide Hours",
                                coverImage: nil
                            )
                        )
                    ),
                    reason: .sideEnded(prompt: prompt)
                )
            )
            return ShotStage(
                Color.clear
                    .background(Theme.surface)
                    .sidePausePromptAlert(showError: { _ in })
                    .environment(store)
                    .environment(Playback.stub())
            )
        }
    }
}
