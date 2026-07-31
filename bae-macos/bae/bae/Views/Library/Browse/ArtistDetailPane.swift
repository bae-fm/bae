import BaeKit
import SwiftUI

/// The artist browse mode's detail pane: the artist header over a grid of their
/// albums. Shows a placeholder before an artist is picked and a spinner while
/// the picked artist's detail loads.
struct ArtistDetailPane: View {
    let detail: BridgeArtistDetail?
    @Environment(LibraryBrowseSession.self)
    private var session
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let detail {
                    BrowseDetailHeader(summary: detail.artist)
                    LazyVGrid(
                        columns: [
                            GridItem(.adaptive(minimum: 140), spacing: 12)
                        ],
                        alignment: .leading,
                        spacing: 12
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
            .padding(24)
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
            .environment(ImageStore.stub)
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
            .environment(ImageStore.stub)
    }
#endif
