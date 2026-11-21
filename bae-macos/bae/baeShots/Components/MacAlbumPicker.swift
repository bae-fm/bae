import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macAlbumPickerScenes: [ShotScene] = [
        albumPickerScene("mac.component.album-picker.new-album", .new),
        albumPickerScene(
            "mac.component.album-picker.library-album",
            libraryAlbum
        ),
    ]

    /// The pop-up of `albumOptions` drawn open in place, its first line on
    /// the pop-up's line: `x` and `y` are where the pop-up's top left is,
    /// in the frame the menu is drawn over.
    static func albumMenu(
        _ album: BridgeDraftAlbum,
        x: CGFloat,
        y: CGFloat
    ) -> some View {
        let menu = MacMenu(
            MacAlbumPicker.albumOptions(album),
            selection: MacAlbumPicker.Choice.standing,
            select: { _ in }
        )
        return MacMenuInPlace(
            { menu.entries },
            highlighted: menu.chosen,
            isPopUp: true
        )
        .offset(x: x - 31, y: y - 6)
    }

    private static func albumPickerScene(
        _ id: String,
        _ album: BridgeDraftAlbum,
        open: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 384,
            height: open ? 146 : 40
        ) {
            ZStack(alignment: .topLeading) {
                MacAlbumPicker(
                    album: album,
                    canEdit: true,
                    actions: inertAlbumActions
                )
                .frame(width: 380, alignment: .leading)
                if open {
                    albumMenu(album, x: 41, y: 0)
                }
            }
        }
    }
}
