import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSeekBar`, with what is held in one stretch and in two.
    static let macSeekBarScenes: [ShotScene] = [
        seekBar(
            "mac.component.seek-bar.playing",
            position: 102.0 / 303.0,
            held: [BridgeTrackSpan(start: 0, end: 0.6)]
        ),
        seekBar(
            "mac.component.seek-bar.holes",
            position: 61.0 / 303.0,
            held: [
                BridgeTrackSpan(start: 0, end: 0.3),
                BridgeTrackSpan(start: 0.55, end: 0.7),
            ]
        ),
    ]

    private static func seekBar(
        _ id: String,
        position: Double,
        held: [BridgeTrackSpan]
    ) -> ShotScene {
        component(id, height: 60) {
            MacSeekBar(
                source: .track(
                    PreviewData.trackPosition(
                        positionMs: Int64((position * 303_000).rounded()),
                        durationMs: 303_000
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
        }
    }
}
