import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumTile`, one shot for each row of its preview.
    static let macAlbumTileScenes: [ShotScene] = [
        albumTileScene("mac.component.album-tile.resting"),
        albumTileScene("mac.component.album-tile.open", open: true),
        albumTileScene("mac.component.album-tile.selected", selected: true),
        albumTileScene("mac.component.album-tile.no-cover", covered: false),
        albumTileScene("mac.component.album-tile.no-year", year: nil),
    ]

    /// The cover a scene's albums show: placeholder art named after the album.
    static func cover(_ title: String) -> BridgeImageRef {
        BridgeImageRef(id: title, version: "1", imageType: .cover)
    }

    private static func albumTileScene(
        _ id: String,
        open: Bool = false,
        selected: Bool = false,
        covered: Bool = true,
        year: Int32? = 1992
    ) -> ShotScene {
        component(
            id,
            width: 174,
            height: 260,
            images: coverStore()
        ) {
            albumTile(
                "Low Tide Hours",
                "Mara Quill",
                year,
                open: open,
                selected: selected,
                covered: covered,
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

    static func albumTile(
        _ title: String,
        _ artist: String,
        _ year: Int32?,
        open: Bool = false,
        selected: Bool = false,
        covered: Bool = true,
        size: CGFloat = 160,
        menu: AlbumCardMenu
    ) -> some View {
        MacAlbumTile(
            title: title,
            artistNames: artist,
            year: year,
            cover: covered ? cover(title) : nil,
            isExpanded: open,
            isSelected: selected,
            size: size,
            menu: menu
        )
    }
}
