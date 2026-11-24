import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTrackColumns`, one shot for each row of its preview: two tracks,
    /// one in each column, then the same under its side's heading.
    static let macTrackColumnsScenes: [ShotScene] = [
        component(
            "mac.component.track-columns.default",
            page: true,
            width: 580,
            height: 100
        ) {
            columns(side: .flat)
        },
        component(
            "mac.component.track-columns.side-heading",
            page: true,
            width: 580,
            height: 100
        ) {
            columns(side: .sided(sideLetter: "A"))
        },
    ]

    /// The two columns of `twoColumnRelease(side:)`.
    private static func columns(side: BridgeTrackSide) -> some View {
        MacTrackColumns(
            release: twoColumnRelease(side: side),
            isCompilation: false,
            currentTrackId: nil,
            isPlaying: false,
            onPlayFromTrack: { _ in },
            onTogglePlayPause: {},
            onAddNext: { _ in },
            onAddToQueue: { _ in },
            onExportTrack: { _ in },
            onRateTrack: { _, _ in },
            onSetTrackHearted: { _, _ in }
        )
        .frame(width: 580, alignment: .leading)
        .environment(UiStore())
    }

    /// The release `MacTrackColumns` lists: two tracks on `side`, one in
    /// each column.
    private static func twoColumnRelease(side: BridgeTrackSide) -> ReleaseDetail
    {
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
                side: side,
                tracks: tracks,
                totalDuration: nil,
                showsTime: false,
                secondColumnStart: 1
            )
        ]
        return ReleaseDetail(bridge: release)
    }
}
