import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page with its Credits and Works under the tracks
    /// (`mac.album.credits`): three roles, one of them on two tracks, and the
    /// two works tracks 4 and 9 perform.
    static let albumCreditsScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.credits",
            size: CGSize(width: 1_200, height: 1_200)
        ) {
            let size = CGSize(width: 1_200, height: 1_200)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                albumCount: 4,
                openAlbum: true,
                detail: creditedBoardAlbum()
            )
            let images = coverGalleryImages()
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    playbackStore: boardPlayback(),
                    images: images,
                    size: size,
                    headerProgress: 1
                ),
                images: images
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        }
    ]

    /// The board's album with the credits and works its board draws.
    private static func creditedBoardAlbum() -> BridgeAlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        var release = detail.releases[0]
        release.hasCredits = true
        release.credits = [
            BridgeCreditRow(
                role: .word("Producer"),
                artists: .named("Ines Varga"),
                tracks: []
            ),
            BridgeCreditRow(
                role: .word("Engineer"),
                artists: .named("Pell Marsh"),
                tracks: []
            ),
            BridgeCreditRow(
                role: .word("Arranger"),
                artists: .named("Ada Lindqvist"),
                tracks: ["2", "5"]
            ),
        ]
        let works: [Int: BridgeTrackWork] = [
            3: BridgeTrackWork(
                workId: "work-ebb",
                title: "Low Tide Suite: I. Ebb",
                composerNames: "Ada Lindqvist"
            ),
            8: BridgeTrackWork(
                workId: "work-waltz",
                title: "Harbor Waltz",
                composerNames: "Ada Lindqvist"
            ),
        ]
        for (index, work) in works {
            release.tracks[index].works = [work]
        }
        release.performsWorks = true
        detail.releases = [release]
        return detail
    }
}
