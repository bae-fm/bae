import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The album page with its rating: the person's four stars over "You ·
    /// Ana rated it 5" (`mac.album.rating`).
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
                (backing.session.albums.list?.totalCount ?? 0) > 0
            }
        }
    ]

    /// The board's album as the person and Ana rated it.
    private static func ratedBoardAlbum() -> BridgeAlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        detail.rating = BridgeAlbumRating(
            canRate: true,
            mine: .four,
            raters: [.you, .person(name: "Ana", stars: .five)]
        )
        return detail
    }
}
