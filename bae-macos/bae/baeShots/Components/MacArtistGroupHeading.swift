import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacArtistGroupHeading` over the artist's two albums, as the grouped
    /// grid lays them out: the heading on its own line, the tiles under it.
    static let macArtistGroupHeadingScenes: [ShotScene] = [
        headingScene(
            "mac.component.artist-group-heading.default",
            picture: cover("Mara Quill")
        ),
        headingScene(
            "mac.component.artist-group-heading.no-image",
            picture: nil
        ),
    ]

    private static func headingScene(
        _ id: String,
        picture: BridgeImageRef?
    ) -> ShotScene {
        component(
            id,
            width: 3 * 200 + 2 * ThemeSpace.page,
            height: 200,
            images: coverStore()
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.page) {
                MacArtistGroupHeading(
                    name: "Mara Quill",
                    picture: picture,
                    countLabel: BridgeCountLabel(
                        key: "core.library.album_count",
                        count: 2
                    )
                )
                HStack(alignment: .top, spacing: ThemeSpace.page) {
                    browseTile("Low Tide Hours", 1992)
                    browseTile("Northbound", 1974)
                }
            }
        }
    }

    private static func browseTile(
        _ title: String,
        _ year: Int32?
    ) -> some View {
        albumTile(
            title,
            nil,
            year,
            width: 200,
            menu: AlbumCardMenu(
                targetCount: 1,
                onPlay: {},
                onAddToQueue: {},
                onAddNext: {},
                onDelete: {}
            )
        )
    }
}
