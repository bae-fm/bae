import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page with its rating: the person's four stars over "You ·
    /// Ana rated it 5", and the stars they gave three of the tracks
    /// (`mac.album.rating`).
    static let albumRatingScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.rating",
            size: CGSize(width: 1_200, height: 1_000)
        ) {
            let size = CGSize(width: 1_200, height: 1_000)
            let uiStore = UiStore()
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                openAlbum: true,
                detail: ratedBoardAlbum()
            )
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    playbackStore: boardPlayback(),
                    size: size
                )
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        }
    ]

    /// The board's album as the person and Ana rated it, with the stars the
    /// person gave tracks 1, 4 and 7.
    private static func ratedBoardAlbum() -> BridgeAlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        detail.rating = BridgeAlbumRating(
            mine: .four,
            raters: [.you, .person(name: "Ana", stars: .five)]
        )
        let stars: [Int: BridgeStars] = [0: .five, 3: .four, 6: .three]
        var release = detail.releases[0]
        for (index, value) in stars {
            release.tracks[index].myStars = value
        }
        release.trackGroups = release.trackGroups.map { group in
            var group = group
            group.tracks = group.tracks.map { track in
                release.tracks.first { $0.id == track.id } ?? track
            }
            return group
        }
        detail.releases = [release]
        return detail
    }
}
