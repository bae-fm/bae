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
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                if let detail {
                    BrowseDetailHeader(summary: detail.artist)
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
                else if session.selectedArtistId == nil {
                    ContentUnavailableView(
                        "Artists",
                        systemImage: "music.mic"
                    )
                }
                else {
                    ProgressView()
                }
            }
            .padding(ThemeSpace.section)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "artistDetail")
        .background(Theme.surface)
    }
}

#if DEBUG
    #Preview("Artist \u{2014} Loaded") {
        let uiStore = UiStore()
        let libraryStore = PreviewData.seededArtistStore()
        let session = PreviewData.browseSession(
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        session.selectArtist("artist-0")
        return ArtistDetailPane(detail: PreviewData.artistDetail)
            .frame(width: 520, height: 640)
            .environment(session)
            .environment(uiStore)
            .environment(ImageStore.stub())
    }

    #Preview("Artist \u{2014} Placeholder") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let session = PreviewData.browseSession(
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        return ArtistDetailPane(detail: nil)
            .frame(width: 520, height: 640)
            .environment(session)
            .environment(uiStore)
            .environment(ImageStore.stub())
    }
#endif
