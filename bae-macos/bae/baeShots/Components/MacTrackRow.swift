import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTrackRow`, a track in an album's list, under each rating system.
    static let macTrackRowScenes: [ShotScene] = [
        trackRow(
            "mac.component.track-row.resting",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            )
        ),
        trackRow(
            "mac.component.track-row.hover",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            hover: true
        ),
        trackRow(
            "mac.component.track-row.current",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            current: true
        ),
        trackRow(
            "mac.component.track-row.hover-on-current",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            hover: true,
            current: true
        ),
        trackRow(
            "mac.component.track-row.compilation",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000,
                displayArtist: "Mara Quill"
            ),
            artist: "Mara Quill"
        ),
        trackRow(
            "mac.component.track-row.picked",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            picked: true
        ),
        trackRow(
            "mac.component.track-row.picked-and-current",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            current: true,
            picked: true
        ),
        trackRow(
            "mac.component.track-row.rated",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            rating: .stars(mine: .four)
        ),
        trackRow(
            "mac.component.track-row.rated-and-hover",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            hover: true,
            rating: .stars(mine: .four)
        ),
        trackRow(
            "mac.component.track-row.heart-system",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            rating: .heart(hearted: true)
        ),
        trackRow(
            "mac.component.track-row.none-system",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            rating: .hidden
        ),
    ]

    private static func trackRow(
        _ id: String,
        track: Track,
        artist: String? = nil,
        hover: Bool = false,
        current: Bool = false,
        picked: Bool = false,
        rating: BridgeTrackRating = .stars(mine: nil)
    ) -> ShotScene {
        component(id, width: 840, height: 70) {
            MacTrackRow(
                track: track,
                artist: artist,
                positionWidth: MacTrackRow.leadingSlotMinWidth,
                isCurrent: current,
                isPlaying: current,
                hover: hover,
                picked: picked,
                rating: rating,
                onPlay: {},
                onTogglePlayPause: {}
            )
            .frame(width: 840, height: artist == nil ? 40 : 52)
        }
    }
}
