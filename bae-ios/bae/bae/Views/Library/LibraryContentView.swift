import BaeKit
import SwiftUI

/// The library's main region below the header: search results when a query is
/// active, otherwise the album grid or the composer/artist browse list for the
/// current mode. Pull-to-refresh triggers a sync and settles briefly so the
/// spinner doesn't snap away before the refreshed rows land.
struct LibraryContentView: View {
    let isSearching: Bool
    let mode: LibraryBrowserMode
    let albumList: AlbumList?
    let composerList: ComposerList?
    let artistList: ArtistList?
    let searchResults: SearchResults?
    let searchError: String?
    let sync: Sync
    let onSelectAlbum: (String) -> Void
    let onSelectComposer: (String) -> Void
    let onSelectArtist: (String) -> Void
    let onSelectWork: (String) -> Void

    var body: some View {
        if isSearching {
            SearchResultsView(
                results: searchResults,
                error: searchError,
                onSelectAlbum: onSelectAlbum,
                onSelectArtist: onSelectArtist,
                onSelectComposer: onSelectComposer,
                onSelectWork: onSelectWork
            )
        }
        else {
            browseContent
                .refreshable {
                    sync.triggerSync()
                    await settleAfterRefresh()
                }
        }
    }

    /// The current tab's list. Every state it can be in — rows, empty, load
    /// failure, and the moment before the list object exists — renders inside a
    /// scroll view (`ListPlaceholder` supplies one for the row-less states), so
    /// the `.refreshable` above always has a scroll view to arm.
    @ViewBuilder
    private var browseContent: some View {
        switch mode {
        case .albums:
            if let albumList {
                AlbumGrid(list: albumList, onSelect: onSelectAlbum)
            }
            else {
                ListPlaceholder { ProgressView() }
            }
        case .composers:
            if let composerList {
                ComposerListView(list: composerList, onSelect: onSelectComposer)
            }
            else {
                ListPlaceholder { ProgressView() }
            }
        case .artists:
            if let artistList {
                ArtistListView(list: artistList, onSelect: onSelectArtist)
            }
            else {
                ListPlaceholder { ProgressView() }
            }
        }
    }

    private func settleAfterRefresh() async {
        do {
            try await Task.sleep(for: .seconds(0.9))
        }
        catch is CancellationError {}
        catch {
            preconditionFailure("Unexpected refresh sleep error: \(error)")
        }
    }
}

#if DEBUG
private struct LibraryContentPreview: View {
    let store = PreviewData.libraryStore()
    var body: some View {
        LibraryContentView(
            isSearching: false,
            mode: .albums,
            albumList: AlbumList.preview(
                albums: PreviewData.albums,
                store: store
            ),
            composerList: nil,
            artistList: nil,
            searchResults: nil,
            searchError: nil,
            sync: .stub(),
            onSelectAlbum: { _ in },
            onSelectComposer: { _ in },
            onSelectArtist: { _ in },
            onSelectWork: { _ in }
        )
        .previewStores(libraryStore: store)
    }
}

#Preview {
    LibraryContentPreview()
}
#endif
