import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacWorkDetail`: a work with one release and one recording.
    static let macWorkDetailScenes: [ShotScene] = [
        component(
            "mac.component.work-detail.default",
            page: true,
            height: 260,
            images: coverStore()
        ) {
            let media = bridgeMediaTerms(media: PreviewData.media(.cd))
            MacWorkDetail(
                detail: BridgeWorkDetail(
                    work: BridgeWorkSummary(
                        workId: "work-suite",
                        title: "Low Tide Suite",
                        composerNames: nil,
                        linkedReleaseCount: 1,
                        representativeCover: nil
                    ),
                    childWorks: [],
                    releases: [
                        BridgeWorkReleaseSummary(
                            releaseId: "rel-blue-room",
                            albumId: "album-blue-room",
                            albumTitle: "Blue Room Sessions",
                            name: .described(year: nil, media: media),
                            media: media,
                            cover: cover("Blue Room Sessions")
                        )
                    ],
                    tracks: [
                        BridgeWorkTrackSummary(
                            trackId: "track-ebb",
                            trackTitle: "Low Tide Suite: I. Ebb",
                            releaseId: "rel-blue-room",
                            albumId: "album-blue-room",
                            albumTitle: "Blue Room Sessions"
                        )
                    ],
                    childWorkCount: 0,
                    releaseCount: 1,
                    trackCount: 1
                ),
                openWork: { _ in },
                openAlbum: { _, _ in }
            )
            .frame(width: 380, alignment: .leading)
        }
    ]
}
