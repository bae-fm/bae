import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacArtistAlbumTile`, one shot for each row of its preview.
    static let macArtistAlbumTileScenes: [ShotScene] = [
        artistAlbumTileScene("mac.component.artist-album-tile.default"),
        artistAlbumTileScene(
            "mac.component.artist-album-tile.unplayable",
            firstUnplayable: true
        ),
    ]

    /// The preview's wide cell, which its grid fills with five columns of at
    /// least 140 pt, the group space apart.
    private static let gridWidth: CGFloat = 842
    private static let tileWidth = (gridWidth - 4 * ThemeSpace.group) / 5

    private static func artistAlbumTileScene(
        _ id: String,
        firstUnplayable: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: gridWidth,
            height: 202,
            images: coverStore()
        ) {
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                artistAlbumCard(
                    "Low Tide Hours",
                    1992,
                    unplayable: firstUnplayable
                )
                artistAlbumCard("Field Recordings", 1979)
                artistAlbumCard("Northbound", 1974)
            }
        }
    }

    private static func artistAlbumCard(
        _ title: String,
        _ year: Int32,
        unplayable: Bool = false
    ) -> some View {
        MacArtistAlbumTile(
            entry: artistAlbum(title, year, unplayable: unplayable),
            onTap: {}
        )
        .frame(width: tileWidth)
    }
}
