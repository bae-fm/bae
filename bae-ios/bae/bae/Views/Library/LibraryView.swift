import BaeKit
import Combine
import SwiftUI

enum LibraryBrowserMode {
    case albums
    case composers
    case artists
}

private enum LibraryRoute: Hashable {
    case album(
        albumId: String,
        initialReleaseId: String?,
        entry: AlbumDetailEntry
    )
    case composer(String)
    case work(String)
    case artist(String)
    /// What sync is waiting on, opened from the sync activity list.
    case syncPending
}

/// Maps a `LibraryRoute` to its pushed screen, threading navigation callbacks
/// back through the shared route path so a destination can open further ones.
private struct LibraryRouteDestination: View {
    let route: LibraryRoute
    @Binding
    var routePath: [LibraryRoute]

    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        switch route {
        case .syncPending:
            if let location = configStore.storage {
                PendingRecordsScreen(location: location)
            }
        case .album(let albumId, let releaseId, let entry):
            AlbumDetailView(
                albumId: albumId,
                initialReleaseId: releaseId,
                entry: entry
            )
        case .composer(let artistId):
            ComposerDetailScreen(
                artistId: artistId,
                openWork: { routePath.append(.work($0)) },
                openAlbum: { albumId, releaseId in
                    routePath.appendAlbum(
                        albumId: albumId,
                        initialReleaseId: releaseId,
                        entry: .album
                    )
                }
            )
        case .work(let workId):
            WorkDetailScreen(
                workId: workId,
                openWork: { routePath.append(.work($0)) },
                openAlbum: { release in
                    routePath.appendAlbum(
                        albumId: release.albumId,
                        initialReleaseId: release.releaseId,
                        entry: .workRelease
                    )
                }
            )
        case .artist(let artistId):
            ArtistDetailScreen(
                artistId: artistId,
                openAlbum: { albumId in
                    routePath.appendAlbum(
                        albumId: albumId,
                        initialReleaseId: nil,
                        entry: .album
                    )
                }
            )
        }
    }
}

/// Library browse root: albums, composers or artists, search, and navigation
/// into their detail screens, above the now-playing bar.
struct LibraryView: View {
    @Environment(LibraryProjectionStore.self)
    private var libraryProjections
    @Environment(LibraryListsStore.self)
    private var libraryLists
    @Environment(Sync.self)
    private var sync
    @Environment(Playback.self)
    private var playback

    @State
    private var showSettings = false
    @State
    private var showDownloads = false
    @State
    private var mode: LibraryBrowserMode = .albums
    @State
    private var routePath: [LibraryRoute] = []
    @State
    private var searchQuery = ""
    // Newest first by default, matching the desktop library.
    @State
    private var sortField = BridgeSortField.dateAdded
    @State
    private var sortDirection = BridgeSortDirection.descending
    @State
    private var albumGrouping = AlbumGrouping()
    @State
    private var composerSortCriterion =
        BridgeComposerSortCriterion(field: .name, direction: .ascending)
    @State
    private var artistSortCriterion =
        BridgeArtistSortCriterion(field: .name, direction: .ascending)

    private var listSelection: LibraryListSelection {
        switch mode {
        case .albums:
            .albums(
                sort: [
                    BridgeSortCriterion(field: sortField, direction: sortDirection)
                ],
                groupByArtist: albumGrouping.byArtist
            )
        case .composers:
            .composers(composerSortCriterion)
        case .artists:
            .artists(artistSortCriterion)
        }
    }

    var body: some View {
        NavigationStack(path: $routePath) {
            VStack(spacing: 0) {
                LibraryBanner()
                LibraryModePicker(mode: $mode)
                content
            }
            .background(Theme.background)
            .navigationDestination(for: LibraryRoute.self) { route in
                LibraryRouteDestination(route: route, routePath: $routePath)
            }
            .navigationTitle("bae")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(
                text: $searchQuery,
                prompt: "Search"
            )
            .onChange(of: searchQuery, initial: true) { _, newQuery in
                libraryProjections.activateSearch(newQuery)
            }
            .onDisappear {
                libraryProjections.deactivateSearch()
            }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    switch mode {
                    case .albums:
                        AlbumSortMenu(
                            sortField: $sortField,
                            sortDirection: $sortDirection,
                            grouping: albumGrouping
                        )
                    case .composers:
                        ComposerSortMenu(criterion: $composerSortCriterion)
                    case .artists:
                        ArtistSortMenu(criterion: $artistSortCriterion)
                    }
                }
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        playback.playLibraryShuffled()
                    } label: {
                        Image(systemName: "shuffle")
                    }
                    .accessibilityLabel(Text("Shuffle Library"))
                }
                ToolbarItem(placement: .topBarTrailing) {
                    SyncActivityToolbarButton(
                        onOpenSettings: { routePath.append(.syncPending) },
                        onOpenDownloads: { showDownloads = true }
                    )
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        showSettings = true
                    } label: {
                        Image(systemName: "gearshape")
                    }
                    .accessibilityLabel("Settings")
                }
            }
            .sheet(isPresented: $showSettings) {
                SettingsView()
            }
            .sheet(isPresented: $showDownloads) {
                DownloadsView()
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                NowPlayingBar()
            }
        }
        .onChange(of: listSelection, initial: true) { _, _ in
            switch mode {
            case .albums:
                libraryLists.updateAlbums(
                    [
                        BridgeSortCriterion(
                            field: sortField,
                            direction: sortDirection
                        )
                    ],
                    groupByArtist: albumGrouping.byArtist
                )
            case .composers:
                libraryLists.updateComposers([composerSortCriterion])
            case .artists:
                libraryLists.updateArtists([artistSortCriterion])
            }
        }
    }
}

extension LibraryView {
    private var isSearching: Bool {
        !searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    @ViewBuilder
    private var content: some View {
        LibraryContentView(
            isSearching: isSearching,
            mode: mode,
            albumList: libraryLists.albums,
            groupAlbumsByArtist: albumGrouping.byArtist,
            composerList: libraryLists.composers,
            artistList: libraryLists.artists,
            searchResults: libraryProjections.search.value,
            searchError: libraryProjections.search.error,
            sync: sync,
            onSelectAlbum: {
                routePath.appendAlbum(
                    albumId: $0,
                    initialReleaseId: nil,
                    entry: .album
                )
            },
            onSelectComposer: { routePath.append(.composer($0)) },
            onSelectArtist: { routePath.append(.artist($0)) },
            onSelectWork: { routePath.append(.work($0)) }
        )
    }

}

private struct LibraryBanner: View {
    @Environment(ConfigStore.self)
    private var configStore

    // Group, so with nothing to show nothing takes the padding.
    var body: some View {
        Group {
            if let error = configStore.lastError {
                HStack(alignment: .top, spacing: ThemeSpace.related) {
                    ErrorDetailDisclosure(error: error)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Button {
                        configStore.clearError()
                    } label: {
                        Image(systemName: "xmark")
                            .themeIcon(.small)
                    }
                    .accessibilityLabel("Dismiss")
                }
                .notice(.danger)
            }
            LibraryRemovalNotice()
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
    }
}

private struct LibraryModePicker: View {
    @Binding
    var mode: LibraryBrowserMode

    var body: some View {
        Picker("Library", selection: $mode) {
            Text("Albums").tag(LibraryBrowserMode.albums)
            Text("Composers").tag(LibraryBrowserMode.composers)
            Text("Artists").tag(LibraryBrowserMode.artists)
        }
        .pickerStyle(.segmented)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
    }
}

private enum LibraryListSelection: Hashable {
    case albums(sort: [BridgeSortCriterion], groupByArtist: Bool)
    case composers(BridgeComposerSortCriterion)
    case artists(BridgeArtistSortCriterion)
}

private extension Array where Element == LibraryRoute {
    mutating func appendAlbum(
        albumId: String,
        initialReleaseId: String?,
        entry: AlbumDetailEntry
    ) {
        append(
            .album(
                albumId: albumId,
                initialReleaseId: initialReleaseId,
                entry: entry
            )
        )
    }
}

#if DEBUG
#Preview {
    LibraryView()
        .previewStores()
}
#endif
