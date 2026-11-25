import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macCoverTileScenes: [ShotScene] = [
        coverTileScene("mac.component.cover-tile.resting", name: "cover.jpg"),
        coverTileScene(
            "mac.component.cover-tile.selected",
            name: "cover.jpg",
            selected: true
        ),
        coverTileScene(
            "mac.component.cover-tile.long-name",
            name: "cover-front-scan-600dpi.jpg"
        ),
    ]

    private static func coverTileScene(
        _ id: String,
        name: String,
        selected: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 130,
            height: 176,
            images: coverImages()
        ) {
            MacCoverTile(
                item: CoverItem(gallery: coverEntry(name)),
                selected: selected,
                onFocus: {},
                onOpenLightbox: {}
            )
            .frame(width: 130)
        }
    }
}
