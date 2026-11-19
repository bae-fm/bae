import BaeKit
import SwiftUI

/// The artist mode's detail pane: the artist's header over their albums.
struct ArtistDetailPane: View {
    let detail: BridgeArtistDetail?
    @Environment(LibraryBrowseSession.self)
    private var session

    var body: some View {
        if detail == nil, session.selectedArtistId == nil {
            // Nothing chosen: the placeholder fills the pane, centred.
            PlaceholderTitle("Artists")
                .background(Theme.surface)
        }
        else {
            page
        }
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                if let detail {
                    MacDetailHeader(summary: detail.artist)
                    MacArtistOtherNames(
                        artistId: detail.artist.artistId,
                        names: detail.creditNames
                    )
                    MacArtistAlbumGrid(albums: detail.albums)
                }
                // A selected artist's read is local: nothing shows until it
                // lands.
            }
            .padding(ThemeSpace.section)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "artistDetail")
        .background(Theme.surface)
    }
}
