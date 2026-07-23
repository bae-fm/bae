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
            switch mode {
            case .albums:
                albumContent
            case .composers:
                composerContent
            case .artists:
                artistContent
            }
        }
    }

    @ViewBuilder
    private var albumContent: some View {
        if let albumList {
            AlbumGrid(list: albumList, onSelect: onSelectAlbum)
                .refreshable {
                    sync.triggerSync()
                    await settleAfterRefresh()
                }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @ViewBuilder
    private var composerContent: some View {
        if let composerList {
            ComposerListView(list: composerList, onSelect: onSelectComposer)
                .refreshable {
                    sync.triggerSync()
                    await settleAfterRefresh()
                }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @ViewBuilder
    private var artistContent: some View {
        if let artistList {
            ArtistListView(list: artistList, onSelect: onSelectArtist)
                .refreshable {
                    sync.triggerSync()
                    await settleAfterRefresh()
                }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
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
