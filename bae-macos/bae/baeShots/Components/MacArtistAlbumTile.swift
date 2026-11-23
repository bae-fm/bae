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

    private static func artistAlbumTileScene(
        _ id: String,
        firstUnplayable: Bool = false
    ) -> ShotScene {
        component(
            id,
            page: true,
            width: 524,
            height: 250,
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
        .frame(width: 160)
    }
}
