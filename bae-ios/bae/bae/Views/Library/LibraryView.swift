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
    /// What sync is waiting on, opened from the library's status line.
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
                LibraryBanner { routePath.append(.syncPending) }
                LibraryModePicker(mode: $mode)
                DownloadsStrip { showDownloads = true }
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
                    LibrarySyncToolbarStatus()
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

/// Sync's state at a glance for a library with a cloud home: a spinner while a
/// cycle runs, otherwise a symbol for core's indicator, read aloud as the
/// status line Settings shows.
private struct LibrarySyncToolbarStatus: View {
    @Environment(SyncStatusStore.self)
    private var syncStatusStore
    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        Group {
            if syncStatusStore.syncing
                || syncStatusStore.indicator == .loading
            {
                ProgressView()
                    .controlSize(.small)
            }
            else {
                Image(systemName: symbol)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement()
        .accessibilityLabel(Text(syncStatusStore.statusText))
        .opacity(configStore.hasCloudHome ? 1 : 0)
        .accessibilityHidden(!configStore.hasCloudHome)
    }

    private var symbol: String {
        switch syncStatusStore.indicator {
        case .synced, .loading: "checkmark.icloud"
        case .waiting: "arrow.clockwise.icloud"
        case .unconnected: "icloud.slash"
        }
    }
}

/// A one-line download summary that opens the Downloads sheet; hidden when the
/// queue is empty.
private struct DownloadsStrip: View {
    let onTap: () -> Void

    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        let snapshot = downloadStore.snapshot
        if !snapshot.downloads.isEmpty {
            Button(action: onTap) {
                HStack(spacing: ThemeSpace.group) {
                    Image(systemName: "arrow.down.circle")
                        .foregroundStyle(.secondary)
                    VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                        DownloadQueueSummaryLine(snapshot: snapshot, compact: true)
                        if let progress = activeProgress(snapshot) {
                            ProgressView(value: progress.fraction)
                                .progressViewStyle(.linear)
                        }
                    }
                    Image(systemName: "chevron.right")
                        .themeIcon(.small)
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, ThemeSpace.edge)
                .padding(.vertical, ThemeSpace.related)
                .contentShape(Rectangle())
                .background(Theme.surface)
            }
            .buttonStyle(.plain)
        }
    }

    private func activeProgress(
        _ snapshot: BridgeDownloadSnapshot
    ) -> BridgeDownloadTransferProgress? {
        for op in snapshot.downloads {
            if case .active(let progress) = op.state {
                return progress
            }
        }
        return nil
    }
}

private struct LibraryBanner: View {
    /// Opens what sync is waiting on.
    let onOpenSyncStatus: () -> Void

    @Environment(ConfigStore.self)
    private var configStore
    @Environment(SyncStatusStore.self)
    private var syncStatusStore

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
            // Disconnecting leaves the recorded sync error behind, so show the
            // line only while the library has a cloud home.
            else if configStore.hasCloudHome, syncStatusStore.needsAttention {
                SyncStatusRow(onOpen: onOpenSyncStatus)
            }
            LibraryRemovalNotice()
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
    }
}

/// The sync status line — "Waiting: 3 items", or that sync is failing and
/// why — that opens what sync is waiting on, where each record has its
/// action.
private struct SyncStatusRow: View {
    let onOpen: () -> Void

    @Environment(SyncStatusStore.self)
    private var syncStatusStore

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: ThemeSpace.related) {
                VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                    Text(syncStatusStore.statusText)
                        .themeText(.strong)
                    if let error = syncStatusStore.error {
                        Text(error.line)
                            .themeText(.detail)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .themeIcon(.small)
                    .foregroundStyle(.secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .notice(.warning)
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
