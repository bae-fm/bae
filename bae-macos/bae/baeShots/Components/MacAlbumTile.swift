import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumTile`, one shot for each row of its preview.
    static let macAlbumTileScenes: [ShotScene] = [
        albumTileScene("mac.component.album-tile.resting"),
        albumTileScene("mac.component.album-tile.unplayable", unplayable: true),
        albumTileScene("mac.component.album-tile.hover", hover: true),
        albumTileScene("mac.component.album-tile.open", open: true),
        albumTileScene("mac.component.album-tile.selected", selected: true),
        albumTileScene("mac.component.album-tile.no-cover", covered: false),
        albumTileScene("mac.component.album-tile.no-year", year: nil),
        albumTileScene("mac.component.album-tile.no-artists", artist: nil),
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
        unplayable: Bool = false,
        hover: Bool = false,
        year: Int32? = 1992,
        artist: String? = "Mara Quill"
    ) -> ShotScene {
        // The preview puts the tile in the first of six grid columns.
        let columns: CGFloat = 6
        let width =
            (wideColumn - 2 * ThemeSpace.edge - (columns - 1) * ThemeSpace.page)
            / columns
        return component(
            id,
            width: wideColumn,
            height: 184,
            images: coverStore()
        ) {
            albumTile(
                "Low Tide Hours",
                artist,
                year,
                open: open,
                selected: selected,
                covered: covered,
                unplayable: unplayable,
                hover: hover,
                width: width,
                menu: AlbumCardMenu(
                    targetCount: 1,
                    onPlay: {},
                    onAddToQueue: {},
                    onAddNext: {},
                    onDelete: {}
                )
            )
            .padding(.horizontal, ThemeSpace.edge)
            .padding(.bottom, ThemeSpace.edge)
        }
    }

    static func albumTile(
        _ title: String,
        _ artist: String?,
        _ year: Int32?,
        open: Bool = false,
        selected: Bool = false,
        covered: Bool = true,
        unplayable: Bool = false,
        hover: Bool = false,
        width: CGFloat = 160,
        menu: AlbumCardMenu
    ) -> some View {
        MacAlbumTile(
            title: title,
            artistNames: artist,
            year: year,
            cover: covered ? cover(title) : nil,
            isExpanded: open,
            isSelected: selected,
            isUnplayable: unplayable,
            isHovered: hover,
            width: width,
            menu: menu
        )
    }
}
