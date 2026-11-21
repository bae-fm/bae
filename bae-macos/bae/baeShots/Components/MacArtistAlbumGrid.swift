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
                gridAlbum("Low Tide Hours", "Mara Quill", 1992),
                gridAlbum("Field Recordings", "Mara Quill", 1979),
                gridAlbum("Northbound", "Mara Quill", 1974),
            ])
            .frame(width: 524, alignment: .leading)
            .environment(UiStore())
        }
    ]
}
