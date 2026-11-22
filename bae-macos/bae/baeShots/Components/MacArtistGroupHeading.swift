import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacArtistGroupHeading` over the artist's two albums, as the grouped
    /// grid lays them out: the heading on its own line, the tiles under it.
    static let macArtistGroupHeadingScenes: [ShotScene] = [
        component(
            "mac.component.artist-group-heading.default",
            width: 3 * 200 + 2 * ThemeSpace.page,
            height: 200,
            images: coverStore()
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.page) {
                MacArtistGroupHeading(name: "Mara Quill")
                HStack(alignment: .top, spacing: ThemeSpace.page) {
                    browseTile("Low Tide Hours", "Mara Quill", 1992)
                    browseTile("Northbound", "Mara Quill", 1974)
                }
            }
        }
    ]

    private static func browseTile(
        _ title: String,
        _ artist: String,
        _ year: Int32?
    ) -> some View {
        albumTile(
            title,
            artist,
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
