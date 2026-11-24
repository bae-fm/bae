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

    /// The preview's wide cell inside its edge space, which the three columns of tiles share with the
    /// page's gap between them.
    private static let pageWidth: CGFloat = 842
    private static let tileWidth = (pageWidth - 2 * ThemeSpace.page) / 3

    private static func headingScene(
        _ id: String,
        picture: BridgeImageRef?
    ) -> ShotScene {
        component(
            id,
            width: pageWidth + 2 * ThemeSpace.edge,
            height: 376,
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
            // The board's cell sets the grid in by the edge space at each
            // side and none above.
            .padding(.horizontal, ThemeSpace.edge)
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
            width: tileWidth,
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
