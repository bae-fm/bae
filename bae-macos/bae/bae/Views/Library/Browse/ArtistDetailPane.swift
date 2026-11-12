import BaeKit
import SwiftUI

/// The artist mode's detail pane: the artist's header over their albums.
struct ArtistDetailPane: View {
    let detail: BridgeArtistDetail?
    @Environment(LibraryBrowseSession.self)
    private var session
    @Environment(UiStore.self)
    private var uiStore

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
                    BrowseDetailHeader(summary: detail.artist)
                    ArtistCreditNamesSection(
                        artistId: detail.artist.artistId,
                        names: detail.creditNames
                    )
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
                        ForEach(detail.albums) { album in
                            ArtistAlbumCard(album: album) {
                                uiStore.navigateToAlbum(album.id)
                            }
                        }
                    }
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
