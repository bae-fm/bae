import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macAlbumArtistFieldScenes: [ShotScene] = [
        albumArtistFieldScene(
            "mac.component.album-artist-field.library-artist",
            artists: PreviewData.boardLibraryArtists("Mara Quill")
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.new-artist",
            artists: newArtists
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.no-artist",
            artists: nil
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.secondary",
            artists: PreviewData.boardLibraryArtists("Mara Quill")
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.column",
            artists: PreviewData.boardLibraryArtists("Mara Quill")
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.inherited",
            artists: PreviewData.boardLibraryArtists("Mara Quill")
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.column-with-no-artist",
            artists: nil
        ),
    ]

    private static func albumArtistFieldScene(
        _ id: String,
        artists: BridgeArtistField?
    ) -> ShotScene {
        component(
            id,
            width: 284,
            height: 40
        ) {
            importStores(
                MacAlbumArtistField(
                    field: artists,
                    placeholder: "Album artist",
                    artistSearch: artistSearch(),
                    onChange: { _ in }
                )
                .frame(width: 284, alignment: .leading)
            )
        }
    }
}
