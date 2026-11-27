import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macAlbumArtistFieldScenes: [ShotScene] = [
        albumArtistFieldScene(
            "mac.component.album-artist-field.library-artist",
            artist: .own(
                field: PreviewData.boardLibraryArtists("Mara Quill")
            )
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.new-artist",
            artist: .own(field: newArtists)
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.no-artist",
            artist: .empty
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.secondary",
            artist: .own(
                field: PreviewData.boardLibraryArtists("Mara Quill")
            ),
            secondary: true
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.column",
            artist: .own(
                field: PreviewData.boardLibraryArtists("Mara Quill")
            ),
            column: true
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.inherited",
            artist: .albumArtist,
            column: true
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.column-with-no-artist",
            artist: .empty,
            placeholder: "Artist",
            column: true
        ),
    ]

    /// One `MacAlbumArtistField`: on a header line as wide as its value, or
    /// (`column`) filling a 240pt table column; `secondary` is the album
    /// identity's text.
    private static func albumArtistFieldScene(
        _ id: String,
        artist: BridgeTrackArtistDisplay,
        placeholder: String = "Album artist",
        secondary: Bool = false,
        column: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: column ? 240 : 284,
            height: 40
        ) {
            importStores(
                MacAlbumArtistField(
                    artist: artist,
                    placeholder: placeholder,
                    width: column ? .column : .value,
                    artistSearch: artistSearch(),
                    onChange: { _ in }
                )
                .themeText(.body)
                .foregroundStyle(
                    secondary ? Theme.textSecondary : Theme.textPrimary
                )
                .frame(width: column ? 240 : 284, alignment: .leading)
            )
        }
    }
}
