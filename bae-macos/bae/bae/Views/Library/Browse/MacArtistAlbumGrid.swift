import BaeKit
import SwiftUI

/// An artist page's grid of the artist's albums, each a `MacArtistAlbumTile`
/// that opens the album.
struct MacArtistAlbumGrid: View {
    let albums: [BridgeAlbum]
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        LazyVGrid(
            columns: [
                GridItem(
                    .adaptive(minimum: 140),
                    spacing: ThemeSpace.group
                )
            ],
            alignment: .leading,
            spacing: ThemeSpace.group
        ) {
            ForEach(albums) { album in
                MacArtistAlbumTile(album: album) {
                    uiStore.navigateToAlbum(album.id)
                }
            }
        }
    }
}
