import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPlayPause`: paused, playing, and waiting for the track to arrive.
    static let macPlayPauseScenes: [ShotScene] = [
        playPause(
            "mac.component.play-pause.paused",
            playing: false,
            waiting: false
        ),
        playPause(
            "mac.component.play-pause.playing",
            playing: true,
            waiting: false
        ),
        playPause(
            "mac.component.play-pause.waiting",
            playing: false,
            waiting: true
        ),
    ]

    private static func playPause(
        _ id: String,
        playing: Bool,
        waiting: Bool
    ) -> ShotScene {
        component(id, height: 70) {
            MacPlayPause(
                isPlaying: playing,
                waitsOnStorage: waiting,
                onToggle: {}
            )
        }
    }
}
