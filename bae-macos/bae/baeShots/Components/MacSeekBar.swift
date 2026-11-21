import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSeekBar`, with what is held in one stretch, in two, and off.
    static let macSeekBarScenes: [ShotScene] = [
        seekBar(
            "mac.component.seek-bar.playing",
            position: 0.35,
            positionMs: 102_000,
            held: [BridgeTrackSpan(start: 0, end: 0.6)]
        ),
        seekBar(
            "mac.component.seek-bar.holes",
            position: 61.0 / 303.0,
            positionMs: 61_000,
            held: [
                BridgeTrackSpan(start: 0, end: 0.3),
                BridgeTrackSpan(start: 0.55, end: 0.7),
            ]
        ),
        seekBar(
            "mac.component.seek-bar.off",
            position: 0.35,
            positionMs: 102_000,
            held: [BridgeTrackSpan(start: 0, end: 0.6)],
            off: true
        ),
    ]

    private static func seekBar(
        _ id: String,
        position: Double,
        positionMs: Int64,
        held: [BridgeTrackSpan],
        off: Bool = false
    ) -> ShotScene {
        component(id, height: 60) {
            MacSeekBar(
                source: .track(
                    PreviewData.trackPosition(
                        positionMs: positionMs,
                        durationMs: 303_000,
                        progress: position
                    ),
                    buffered:
                        LatestStream.trackBuffered(
                            holding: [
                                "preview-track": BridgeTrackBuffered(
                                    ranges: held
                                )
                            ]
                        )
                        .source(for: "preview-track")
                ),
                showRemainingTime: false,
                onSeek: { _ in },
                onToggleRemainingTime: nil
            )
            .frame(width: 380, height: 20)
            .disabled(off)
        }
    }
}
