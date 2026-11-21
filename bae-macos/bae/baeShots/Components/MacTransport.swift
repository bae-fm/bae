import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTransport`: paused, playing, waiting on storage, with shuffle on
    /// and with repeat on.
    static let macTransportScenes: [ShotScene] = [
        transport("mac.component.transport.paused"),
        transport("mac.component.transport.playing", playing: true),
        transport(
            "mac.component.transport.waiting",
            playing: true,
            waiting: true
        ),
        transport("mac.component.transport.shuffle", shuffled: true),
        transport("mac.component.transport.repeat", repeatMode: .context),
    ]

    private static func transport(
        _ id: String,
        playing: Bool = false,
        waiting: Bool = false,
        shuffled: Bool = false,
        repeatMode: BridgeRepeatMode = .off
    ) -> ShotScene {
        component(id, width: 264, height: 60) {
            MacTransport(
                isPlaying: playing,
                waitsOnStorage: waiting,
                shuffled: shuffled,
                repeatMode: repeatMode,
                onPlayPause: {},
                onNext: {},
                onPrevious: {},
                onSetShuffle: { _ in },
                onSetRepeatMode: { _ in }
            )
        }
    }
}
