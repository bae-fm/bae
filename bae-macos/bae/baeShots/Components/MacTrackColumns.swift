import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTrackColumns`: two tracks, one in each column.
    static let macTrackColumnsScenes: [ShotScene] = [
        component(
            "mac.component.track-columns.default",
            page: true,
            width: 580,
            height: 100
        ) {
            MacTrackColumns(
                release: twoColumnRelease(),
                isCompilation: false,
                currentTrackId: nil,
                isPlaying: false,
                onPlayFromTrack: { _ in },
                onTogglePlayPause: {},
                onAddNext: { _ in },
                onAddToQueue: { _ in },
                onExportTrack: { _ in },
                onRateTrack: { _, _ in }
            )
            .frame(width: 580, alignment: .leading)
            .environment(UiStore())
        }
    ]

    /// The release `MacTrackColumns` lists: two tracks, one in each column.
    private static func twoColumnRelease() -> ReleaseDetail {
        var release = PreviewData.boardAlbumDetail().releases[0]
        let tracks = [
            browseTrack(
                "t1",
                "Harbor Lights",
                position: "1",
                minutes: 4,
                seconds: 32
            ),
            browseTrack(
                "t6",
                "Low Tide Hours",
                position: "6",
                minutes: 5,
                seconds: 3
            ),
        ]
        release.tracks = tracks
        release.trackGroups = [
            BridgeTrackGroup(
                side: .flat,
                tracks: tracks,
                totalDuration: nil,
                showsTime: false,
                secondColumnStart: 1
            )
        ]
        return ReleaseDetail(bridge: release)
    }
}
