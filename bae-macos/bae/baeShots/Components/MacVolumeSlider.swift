import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacVolumeSlider` at 70 %, silent and full, and the line that says
    /// where a receiver without volume sets it.
    static let macVolumeSliderScenes: [ShotScene] = [
        volumeSlider("mac.component.volume-slider.resting", volume(0.7)),
        volumeSlider("mac.component.volume-slider.silent", volume(0)),
        volumeSlider("mac.component.volume-slider.full", volume(1)),
        volumeSlider(
            "mac.component.volume-slider.no-volume",
            .setOnReceiver("Living Room")
        ),
    ]

    private static func volumeSlider(
        _ id: String,
        _ controls: MacVolumeSlider.Controls
    ) -> ShotScene {
        component(id, width: 184, height: 60) {
            MacVolumeSlider(
                controls: controls,
                onDrag: { _ in },
                onChange: { _ in },
                onSetMuted: { _ in }
            )
        }
    }

    private static func volume(_ level: Float) -> MacVolumeSlider.Controls {
        .controls(
            PlaybackSettings(volume: level, isMuted: false, repeatMode: .off)
        )
    }
}
