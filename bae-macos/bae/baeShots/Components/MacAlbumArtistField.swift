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
            artists: PreviewData.boardLibraryArtists("Mara Quill"),
            secondary: true
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.column",
            artists: PreviewData.boardLibraryArtists("Mara Quill"),
            column: true
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.inherited",
            artists: nil,
            column: true,
            inherited: true
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.column-with-no-artist",
            artists: nil,
            placeholder: "Artist",
            column: true
        ),
    ]

    /// One `MacAlbumArtistField`: on a header line as wide as its value, or
    /// (`column`) filling a 240pt table column; `secondary` is the album
    /// identity's text.
    private static func albumArtistFieldScene(
        _ id: String,
        artists: BridgeArtistField?,
        placeholder: String = "Album artist",
        secondary: Bool = false,
        column: Bool = false,
        inherited: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: column ? 240 : 284,
            height: 40
        ) {
            importStores(
                MacAlbumArtistField(
                    field: artists,
                    placeholder: placeholder,
                    width: column ? .column : .value,
                    inheritsAlbumArtists: inherited,
                    artistSearch: artistSearch(),
                    onChange: { _ in }
                )
                .themeText(.body)
                .foregroundStyle(
                    secondary ? Theme.macTextSecondary : Theme.textPrimary
                )
                .frame(width: column ? 240 : 284, alignment: .leading)
            )
        }
    }
}
