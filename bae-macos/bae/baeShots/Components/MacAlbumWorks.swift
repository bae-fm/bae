import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumWorks` with two works and with one.
    static let macAlbumWorksScenes: [ShotScene] = [
        albumWorks(
            "mac.component.album-works.two-works",
            height: 130,
            [
                workTrack("4", "Low Tide Suite: I. Ebb"),
                workTrack("9", "Harbor Waltz"),
            ]
        ),
        albumWorks(
            "mac.component.album-works.one-work",
            height: 100,
            [workTrack("4", "Low Tide Suite: I. Ebb")]
        ),
    ]

    private static func workTrack(_ position: String, _ title: String)
        -> Track
    {
        var track = PreviewData.previewTrack(
            id: "track-\(position)",
            title: title,
            position: position
        )
        track.works = [
            BridgeTrackWork(
                workId: "work-\(position)",
                title: title,
                composerNames: "Ada Lindqvist"
            )
        ]
        return track
    }

    private static func albumWorks(
        _ id: String,
        height: CGFloat,
        _ tracks: [Track]
    ) -> ShotScene {
        component(id, width: 480, height: height) {
            MacAlbumWorks(tracks: tracks, onOpenWork: { _ in })
                .frame(width: 480, alignment: .leading)
        }
    }
}
