import BaeKit
import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("LibraryView")

/// Page size for the library's paged album/composer/artist lists, shared by
/// every grid/list slot's range fetch.
let libraryPageSize = 60

enum LibraryBrowserMode {
    case albums
    case composers
    case artists
}

private enum LibraryRoute: Hashable {
    case album(
        albumId: String,
        initialReleaseId: String?,
        context: AlbumDetailContext?
    )
    case composer(String)
    case work(String)
    case artist(String)
}

/// Maps a `LibraryRoute` to its pushed screen, threading navigation callbacks
/// back through the shared route path so a destination can open further ones.
private struct LibraryRouteDestination: View {
    let route: LibraryRoute
    @Binding
    var routePath: [LibraryRoute]

    var body: some View {
        switch route {
        case .album(let albumId, let releaseId, let context):
            AlbumDetailView(
                albumId: albumId,
                initialReleaseId: releaseId,
                context: context
            )
        case .composer(let artistId):
            ComposerDetailScreen(
                artistId: artistId,
                openWork: { routePath.append(.work($0)) },
                openAlbum: { albumId, releaseId in
                    routePath.appendAlbum(
                        albumId: albumId,
                        initialReleaseId: releaseId,
                        context: nil
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
                        context: AlbumDetailContext(workRelease: release)
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
                        context: nil
                    )
                }
            )
        }
    }
}

/// Library browse root: a top bar with sync status, an album grid paged from
/// the database, navigation into album detail, and a persistent now-playing
/// bar. The grid receives updated pages from its database subscriptions.
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
    // Newest-first by default, matching the desktop library. Held as the two
    // enum components (each Equatable) so onChange can rebuild the paged list.
    @State
    private var sortField = BridgeSortField.dateAdded
    @State
    private var sortDirection = BridgeSortDirection.descending
    @State
    private var composerSortCriterion =
        BridgeComposerSortCriterion(field: .name, direction: .ascending)
    @State
    private var artistSortCriterion =
        BridgeArtistSortCriterion(field: .name, direction: .ascending)

    private var listSelection: LibraryListSelection {
        switch mode {
        case .albums:
            .albums(field: sortField, direction: sortDirection)
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
            .onChange(of: searchQuery, initial: true) { oldQuery, newQuery in
                libraryProjections.deactivateSearch(oldQuery)
                libraryProjections.activateSearch(newQuery)
            }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    switch mode {
                    case .albums:
                        AlbumSortMenu(
                            sortField: $sortField,
                            sortDirection: $sortDirection
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
                libraryLists.updateAlbums([
                    BridgeSortCriterion(
                        field: sortField,
                        direction: sortDirection
                    )
                ])
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
            composerList: libraryLists.composers,
            artistList: libraryLists.artists,
            searchResults: libraryProjections.search.value,
            searchError: libraryProjections.search.error?.line,
            sync: sync,
            onSelectAlbum: {
                routePath.appendAlbum(
                    albumId: $0,
                    initialReleaseId: nil,
                    context: nil
                )
            },
            onSelectComposer: { routePath.append(.composer($0)) },
            onSelectArtist: { routePath.append(.artist($0)) },
            onSelectWork: { routePath.append(.work($0)) }
        )
    }

}

private struct LibrarySyncToolbarStatus: View {
    @Environment(SyncStatusStore.self)
    private var syncStatusStore

    var body: some View {
        Group {
            // The live spinner is its own element, driven by an in-progress cycle;
            // the badge word comes from core's indicator when no cycle is running.
            if syncStatusStore.syncing {
                ProgressView()
                    .controlSize(.small)
                    .accessibilityLabel(Text("syncing\u{2026}"))
            }
            else {
                Text(SyncIndicatorLabel.text(syncStatusStore.indicator))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: 56, height: 20, alignment: .trailing)
    }
}

/// Compact one-line download summary between the mode picker and the grid:
/// the active download's bar (the queue is serial, so at most one) plus the
/// count summary / paused chip. Tapping opens the Downloads sheet. Hidden
/// when the queue is empty — the queue is transient, so there is nothing to
/// manage then.
private struct DownloadsStrip: View {
    let onTap: () -> Void

    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        let snapshot = downloadStore.snapshot
        if !snapshot.downloads.isEmpty {
            Button(action: onTap) {
                HStack(spacing: 12) {
                    Image(systemName: "arrow.down.circle")
                        .foregroundStyle(.secondary)
                    VStack(alignment: .leading, spacing: 4) {
                        DownloadQueueSummaryLine(snapshot: snapshot, compact: true)
                        if let progress = activeProgress(snapshot) {
                            ProgressView(value: progress.fraction)
                                .progressViewStyle(.linear)
                        }
                    }
                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
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
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(SyncStatusStore.self)
    private var syncStatusStore
    @Environment(Sync.self)
    private var sync

    @State
    private var reconnecting = false

    var body: some View {
        if let error = configStore.lastError {
            banner(message: error.line) {
                Button {
                    configStore.clearError()
                } label: {
                    Image(systemName: "xmark")
                        .font(.caption.bold())
                        .foregroundStyle(Color.white)
                }
                .accessibilityLabel("Dismiss")
            }
        }
        // A sync failure is a failure of the provider this library is
        // configured for. Disconnecting clears that config but leaves the
        // recorded error behind, so without the config check a library with no
        // provider would wear a permanent red retry strip for a provider it no
        // longer has.
        else if let error = syncStatusStore.error, configStore.config.sync != nil {
            banner(message: error.line) {
                if reconnecting {
                    ProgressView()
                        .controlSize(.small)
                        .tint(Color.white)
                }
                else {
                    Button("Retry") { Task { await reconnect() } }
                        .font(.caption.bold())
                        .foregroundStyle(Color.white)
                }
            }
        }
    }

    /// Retry the provider this library is already configured for, connecting
    /// when a failed launch left no connection rather than only waking a loop
    /// that may not exist. A failed retry is recorded as the sync-status error
    /// this banner renders, so it stays up naming the new reason.
    private func reconnect() async {
        reconnecting = true
        do {
            try await sync.reconnectSync()
        }
        catch {
            logger.error(
                "Sync reconnect failed: \(error.localizedDescription)"
            )
        }
        reconnecting = false
    }

    private func banner<Trailing: View>(
        message: String,
        @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack {
            Text(message)
                .font(.caption)
                .foregroundStyle(Color.white)
                .frame(maxWidth: .infinity, alignment: .leading)
            trailing()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(Color.red.opacity(0.7))
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
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }
}

private enum LibraryListSelection: Hashable {
    case albums(field: BridgeSortField, direction: BridgeSortDirection)
    case composers(BridgeComposerSortCriterion)
    case artists(BridgeArtistSortCriterion)
}

private extension Array where Element == LibraryRoute {
    mutating func appendAlbum(
        albumId: String,
        initialReleaseId: String?,
        context: AlbumDetailContext?
    ) {
        append(
            .album(
                albumId: albumId,
                initialReleaseId: initialReleaseId,
                context: context
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
