import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumGrid` over four albums, one row of four columns.
    static let macAlbumGridScenes: [ShotScene] = {
        let images = coverStore()
        return [
            component(
                "mac.component.album-grid.default",
                width: 4 * 200 + 3 * ThemeSpace.page
                    + 2 * LibraryContentContainer.horizontalPadding,
                height: 330,
                images: images
            ) {
                MacAlbumGrid(
                    list: AlbumList.preview(albums: [
                        gridAlbum("Low Tide Hours", "Mara Quill", 1992),
                        gridAlbum("Paper Lanterns", "The Slow Orchards", 1971),
                        gridAlbum("Glass Harbor", "Ines Varga Trio", 1959),
                        gridAlbum("Copper Sky", "Odile Fen", nil),
                    ]),
                    sortCriteria: [],
                    fullWidth: false,
                    selection: AlbumGridSelection(),
                    onPlay: { _ in },
                    onAddToQueue: { _ in },
                    onAddNext: { _ in },
                    onDeleteAlbum: { _ in }
                ) { _ in
                    EmptyView()
                }
                .albumDetailPreviewEnvironment()
                .environment(images)
            }
        ]
    }()

    static func gridAlbum(
        _ title: String,
        _ artist: String,
        _ year: Int32?
    ) -> BridgeAlbum {
        BridgeAlbum(
            id: title,
            title: title,
            year: year,
            isCompilation: false,
            artistLine: .named(artist),
            releaseIds: [],
            primaryReleaseId: title,
            cover: cover(title)
        )
    }
}
