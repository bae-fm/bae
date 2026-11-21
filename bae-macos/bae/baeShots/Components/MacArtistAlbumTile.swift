import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacArtistAlbumTile`, one shot for each row of its preview.
    static let macArtistAlbumTileScenes: [ShotScene] = [
        component(
            "mac.component.artist-album-tile.default",
            page: true,
            width: 524,
            height: 250,
            images: coverStore()
        ) {
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                artistAlbumCard("Low Tide Hours", 1992)
                artistAlbumCard("Field Recordings", 1979)
                artistAlbumCard("Northbound", 1974)
            }
        }
    ]

    private static func artistAlbumCard(
        _ title: String,
        _ year: Int32
    ) -> some View {
        MacArtistAlbumTile(
            album: gridAlbum(title, "Mara Quill", year),
            onTap: {}
        )
        .frame(width: 160)
    }
}
