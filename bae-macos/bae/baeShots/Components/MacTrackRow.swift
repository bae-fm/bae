import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTrackRow`, a track in an album's list, under each rating system.
    static let macTrackRowScenes: [ShotScene] = [
        trackRow(
            "mac.component.track-row.resting",
            track: PreviewData.previewTrack(
                title: "Harbor Lights",
                position: "1",
                durationMs: 272_000
            )
        ),
        trackRow(
            "mac.component.track-row.hover",
            track: PreviewData.previewTrack(
                title: "The Long Way Home",
                position: "2",
                durationMs: 312_000
            ),
            hover: true
        ),
        trackRow(
            "mac.component.track-row.current",
            track: PreviewData.previewTrack(
                title: "Second Floor",
                position: "3",
                durationMs: 225_000
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
            stars: .four
        ),
        trackRow(
            "mac.component.track-row.rated-and-hover",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            hover: true,
            stars: .four
        ),
        trackRow(
            "mac.component.track-row.heart-system",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            stars: .five,
            ratingSystem: .heart
        ),
        trackRow(
            "mac.component.track-row.none-system",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            stars: .four,
            ratingSystem: .hidden
        ),
    ]

    private static func trackRow(
        _ id: String,
        track: Track,
        artist: String? = nil,
        hover: Bool = false,
        current: Bool = false,
        picked: Bool = false,
        stars: BridgeStars? = nil,
        ratingSystem: BridgeRatingSystem = .stars
    ) -> ShotScene {
        component(id, height: 70) {
            MacTrackRow(
                track: track,
                artist: artist,
                positionWidth: MacTrackRow.leadingSlotMinWidth,
                isCurrent: current,
                isPlaying: current,
                hover: hover,
                picked: picked,
                stars: stars,
                ratingSystem: ratingSystem,
                onPlay: {},
                onTogglePlayPause: {}
            )
            .frame(width: 380, height: artist == nil ? 40 : 52)
        }
    }
}
