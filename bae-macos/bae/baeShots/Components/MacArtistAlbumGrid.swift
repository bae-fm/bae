import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacArtistAlbumGrid`, one shot for each row of its preview.
    static let macArtistAlbumGridScenes: [ShotScene] = [
        component(
            "mac.component.artist-album-grid.three-albums",
            width: 524,
            height: 250,
            images: coverStore()
        ) {
            MacArtistAlbumGrid(albums: [
                artistAlbum("Low Tide Hours", 1992),
                artistAlbum("Field Recordings", 1979),
                artistAlbum("Northbound", 1974),
            ])
            .frame(width: 524, alignment: .leading)
            .environment(UiStore())
        }
    ]

    /// An album of the artist's page, which this Mac can play unless
    /// `unplayable`.
    static func artistAlbum(
        _ title: String,
        _ year: Int32,
        unplayable: Bool = false
    ) -> BridgeAlbumAppearance {
        BridgeAlbumAppearance(
            album: gridAlbum(title, "Mara Quill", year),
            primaryReleaseUnplayable: unplayable,
            displayArtistLine: nil
        )
    }
}
