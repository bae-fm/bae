import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSeekBar`, with what is held in one stretch, in two, and off.
    static let macSeekBarScenes: [ShotScene] = [
        seekBar(
            "mac.component.seek-bar.playing",
            source: track(
                at: 102_000,
                held: [BridgeTrackSpan(start: 0, end: 0.6)]
            )
        ),
        seekBar(
            "mac.component.seek-bar.holes",
            source: track(
                at: 102_000,
                held: [
                    BridgeTrackSpan(start: 0, end: 0.3),
                    BridgeTrackSpan(start: 0.55, end: 0.7),
                ]
            )
        ),
        seekBar("mac.component.seek-bar.off", source: nil),
    ]

    private static func track(at positionMs: Int64, held: [BridgeTrackSpan])
        -> SeekBarSource
    {
        .track(
            PreviewData.trackPosition(
                positionMs: positionMs,
                durationMs: 303_000
            ),
            buffered: PreviewData.trackBuffered(held)
        )
    }

    private static func seekBar(_ id: String, source: SeekBarSource?)
        -> ShotScene
    {
        component(id, width: 840, height: 60) {
            MacSeekBar(
                source: source,
                showRemainingTime: false,
                showsIdleClock: true,
                onSeek: { _ in },
                onToggleRemainingTime: nil
            )
            .frame(width: 840, height: 20)
        }
    }
}
