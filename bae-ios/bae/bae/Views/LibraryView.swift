import BaeKit
import Combine
import SwiftUI

private let pageSize = 60

private enum LibraryBrowserMode {
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
/// bar. The grid re-queries when core invalidates the album, artist, or
/// composer list.
struct LibraryView: View {
    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(ProjectionRegistry.self)
    private var projectionRegistry
    @Environment(Library.self)
    private var library
    @Environment(ConfigStore.self)
    private var configStore
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
    private var albumList: AlbumList?
    @State
    private var composerList: ComposerList?
    @State
    private var artistList: ArtistList?
    @State
    private var albumListRegistration: ProjectionRegistration?
    @State
    private var composerListRegistration: ProjectionRegistration?
    @State
    private var artistListRegistration: ProjectionRegistration?
    @State
    private var routePath: [LibraryRoute] = []
    @State
    private var searchQuery = ""
    @State
    private var searchResults: SearchResults?
    @State
    private var searchError: String?
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
                prompt: "Search albums, tracks, composers, and works"
            )
            .task(id: searchQuery) {
                await runSearch()
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
        .task(id: listSelection) {
            switch mode {
            case .albums:
                await rebuildList()
            case .composers:
                await rebuildComposerList()
            case .artists:
                await rebuildArtistList()
            }
        }
    }
}

extension LibraryView {
    private var isSearching: Bool {
        !searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Rebuild the paged album list for the current sort — on first appear and
    /// whenever the sort field/direction changes.
    private func rebuildList() async {
        let newList = AlbumList(
            pageSource: LibraryAlbumPageSource(
                library: library,
                sort: [
                    BridgeSortCriterion(field: sortField, direction: sortDirection)
                ]
            ),
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row)
                }
            },
            onError: { error in
                configStore.showError(error)
            },
        )
        albumListRegistration = projectionRegistry.registerList(
            newList,
            domain: .albumList
        )
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        albumList = newList
    }

    private func rebuildComposerList() async {
        let newList = ComposerList(
            pageSource: LibraryComposerPageSource(
                library: library,
                sort: [composerSortCriterion]
            ),
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internComposerSummary(row)
                }
            },
            onError: { error in
                configStore.showError(error)
            },
        )
        composerListRegistration = projectionRegistry.registerList(
            newList,
            domain: .composerList
        )
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        composerList = newList
    }

    private func rebuildArtistList() async {
        let newList = ArtistList(
            pageSource: LibraryArtistPageSource(
                library: library,
                sort: [artistSortCriterion]
            ),
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internArtistSummary(row)
                }
            },
            onError: { error in
                configStore.showError(error)
            },
        )
        artistListRegistration = projectionRegistry.registerList(
            newList,
            domain: .artistList
        )
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        artistList = newList
    }

    @ViewBuilder
    private var content: some View {
        LibraryContentView(
            isSearching: isSearching,
            mode: mode,
            albumList: albumList,
            composerList: composerList,
            artistList: artistList,
            searchResults: searchResults,
            searchError: searchError,
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

    /// Debounced library search. `.task(id: searchQuery)` cancels the prior run
    /// on each keystroke: a pending 300ms debounce unwinds, and the post-await
    /// `Task.checkCancellation()` drops a superseded query's results before they
    /// are assigned — so only the latest query's results land. Prior results
    /// stay on screen while the next search runs.
    private func runSearch() async {
        let query = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        searchError = nil
        guard !query.isEmpty else {
            searchResults = nil
            return
        }
        do {
            try await Task.sleep(for: .milliseconds(300))
            let bridge = try await library.searchLibrary(query)
            try Task.checkCancellation()
            searchResults = SearchResults(bridge: bridge, query: query)
        }
        catch is CancellationError {
            // Superseded by a newer query (or cleared); leave prior results.
        }
        catch {
            searchError = error.displayLine
        }
    }
}

private struct LibrarySyncToolbarStatus: View {
    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        Group {
            // The live spinner is its own element, driven by an in-progress cycle;
            // the badge word comes from core's indicator when no cycle is running.
            if configStore.syncing {
                ProgressView()
                    .controlSize(.small)
                    .accessibilityLabel(Text("syncing\u{2026}"))
            }
            else {
                Text(syncStatusLabel(configStore.syncIndicator))
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
    @Environment(Sync.self)
    private var sync

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
        else if let error = configStore.syncError {
            banner(message: error.line) {
                Button("Retry") { sync.triggerSync() }
                    .font(.caption.bold())
                    .foregroundStyle(Color.white)
            }
        }
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

private struct AlbumSortMenu: View {
    @Binding
    var sortField: BridgeSortField
    @Binding
    var sortDirection: BridgeSortDirection

    var body: some View {
        Menu {
            ForEach(BridgeSortField.allCases, id: \.self) { field in
                Button {
                    sortField = field
                } label: {
                    if field == sortField {
                        Label(field.displayName, systemImage: "checkmark")
                    }
                    else {
                        Text(field.displayName)
                    }
                }
            }
            Divider()
            Button {
                sortDirection =
                    sortDirection == .ascending ? .descending : .ascending
            } label: {
                Label(
                    sortDirection == .ascending
                        ? String(localized: "Ascending")
                        : String(localized: "Descending"),
                    systemImage: sortDirection == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }
}

private struct ComposerSortMenu: View {
    @Binding
    var criterion: BridgeComposerSortCriterion

    var body: some View {
        Menu {
            ForEach(BridgeComposerSortField.allCases, id: \.self) { field in
                Button {
                    criterion = BridgeComposerSortCriterion(
                        field: field,
                        direction: criterion.direction
                    )
                } label: {
                    if field == criterion.field {
                        Label(field.displayName, systemImage: "checkmark")
                    }
                    else {
                        Text(field.displayName)
                    }
                }
            }
            Divider()
            Button {
                let direction: BridgeSortDirection =
                    criterion.direction == .ascending ? .descending : .ascending
                criterion = BridgeComposerSortCriterion(
                    field: criterion.field,
                    direction: direction
                )
            } label: {
                Label(
                    criterion.direction == .ascending
                        ? String(localized: "Ascending")
                        : String(localized: "Descending"),
                    systemImage: criterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }
}

private struct ArtistSortMenu: View {
    @Binding
    var criterion: BridgeArtistSortCriterion

    var body: some View {
        Menu {
            ForEach(BridgeArtistSortField.allCases, id: \.self) { field in
                Button {
                    criterion = BridgeArtistSortCriterion(
                        field: field,
                        direction: criterion.direction
                    )
                } label: {
                    if field == criterion.field {
                        Label(field.displayName, systemImage: "checkmark")
                    }
                    else {
                        Text(field.displayName)
                    }
                }
            }
            Divider()
            Button {
                let direction: BridgeSortDirection =
                    criterion.direction == .ascending ? .descending : .ascending
                criterion = BridgeArtistSortCriterion(
                    field: criterion.field,
                    direction: direction
                )
            } label: {
                Label(
                    criterion.direction == .ascending
                        ? String(localized: "Ascending")
                        : String(localized: "Descending"),
                    systemImage: criterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }
}

private struct LibraryContentView: View {
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

/// The paged album grid. Renders one card per loaded slot, resolving each
/// album's summary from the store and its cover from the primary release's
/// on-disk file. Rows that aren't loaded yet kick off their page load via
/// `.task(id:)` keyed on the list's `loadEpoch`.
private struct AlbumGrid: View {
    let list: AlbumList
    let onSelect: (String) -> Void

    private let columns = [GridItem(.adaptive(minimum: 150), spacing: 12)]

    var body: some View {
        if let error = list.initialLoadError {
            LoadFailureView(line: error.line) {
                Task { await list.loadInitial() }
            }
        }
        else if list.totalCount == 0 {
            Text("No albums yet. Syncing from the cloud\u{2026}")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 12) {
                    ForEach(0..<list.totalCount, id: \.self) { position in
                        AlbumCell(
                            list: list,
                            position: position,
                            onSelect: onSelect
                        )
                    }
                }
                .padding(12)
            }
        }
    }
}

/// One grid slot. Loads the page covering its position (keyed on `loadEpoch` so
/// a list swap or invalidation re-fetches), then renders the album once its id
/// resolves.
private struct AlbumCell: View {
    let list: AlbumList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let albumId = list.idAt(position),
                let summary = libraryStore.albumSummaries[albumId]
            {
                AlbumCard(
                    summary: summary,
                    onTap: { onSelect(albumId) }
                )
            }
            else {
                Theme.placeholder
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
        }
        .task(id: list.loadEpoch) {
            let offset = (position / pageSize) * pageSize
            await list.loadRange(offset: offset, limit: pageSize)
        }
    }
}

private struct AlbumCard: View {
    let summary: AlbumSummary
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(imageRef: summary.cover, pointSize: 150)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(summary.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(summary.artistNames)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.plain)
        // One VoiceOver element per card: announce "Title by Artist" instead of
        // the cover image + two separate text fragments. The cover is decorative
        // (its info is in the text), so it's folded into the combined label.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            summary.artistNames.isEmpty
                ? summary.title
                : String(
                    localized: "\(summary.title) by \(summary.artistNames)",
                    comment: "Album card VoiceOver label: title by artist"
                )
        )
    }
}

private struct ComposerListView: View {
    let list: ComposerList
    let onSelect: (String) -> Void

    var body: some View {
        if let error = list.initialLoadError {
            LoadFailureView(line: error.line) {
                Task { await list.loadInitial() }
            }
        }
        else if list.totalCount == 0 {
            Text("No composers")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        else {
            List {
                ForEach(0..<list.totalCount, id: \.self) { position in
                    ComposerRowSlot(
                        list: list,
                        position: position,
                        onSelect: onSelect
                    )
                }
            }
            .listStyle(.plain)
        }
    }
}

private struct ComposerRowSlot: View {
    let list: ComposerList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let id = list.idAt(position),
                let summary = libraryStore.composerSummaries[id]
            {
                Button {
                    onSelect(id)
                } label: {
                    ComposerSummaryRow(summary: summary)
                }
                .buttonStyle(.plain)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: list.loadEpoch) {
            let offset = (position / pageSize) * pageSize
            await list.loadRange(offset: offset, limit: pageSize)
        }
    }
}

private struct ComposerSummaryRow: View {
    let summary: BridgeComposerSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.image, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(summary.name)
                    .font(.body)
                    .lineLimit(1)
                Text("\(summary.workCount) \(String(localized: "Works"))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct ArtistListView: View {
    let list: ArtistList
    let onSelect: (String) -> Void

    var body: some View {
        if let error = list.initialLoadError {
            LoadFailureView(line: error.line) {
                Task { await list.loadInitial() }
            }
        }
        else if list.totalCount == 0 {
            Text("No artists")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        else {
            List {
                ForEach(0..<list.totalCount, id: \.self) { position in
                    ArtistRowSlot(
                        list: list,
                        position: position,
                        onSelect: onSelect
                    )
                }
            }
            .listStyle(.plain)
        }
    }
}

private struct ArtistRowSlot: View {
    let list: ArtistList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let id = list.idAt(position),
                let summary = libraryStore.artistSummaries[id]
            {
                Button {
                    onSelect(id)
                } label: {
                    ArtistSummaryRow(summary: summary)
                }
                .buttonStyle(.plain)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: list.loadEpoch) {
            let offset = (position / pageSize) * pageSize
            await list.loadRange(offset: offset, limit: pageSize)
        }
    }
}

private struct ArtistSummaryRow: View {
    let summary: BridgeArtistSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.image, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(summary.name)
                    .font(.body)
                    .lineLimit(1)
                Text("\(summary.albumCount) \(String(localized: "Albums"))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct ComposerDetailScreen: View {
    let artistId: String
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    @Environment(Library.self)
    private var library

    @State
    private var detail: BridgeComposerDetail?
    @State
    private var error: String?

    var body: some View {
        Group {
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .padding(32)
            }
            else if let detail {
                ComposerDetailContent(
                    detail: detail,
                    openWork: openWork,
                    openAlbum: openAlbum
                )
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: artistId) {
            await load()
        }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.composer.name
        }
        if error != nil {
            return String(localized: "Composers")
        }
        return ""
    }

    private func load() async {
        error = nil
        do {
            let getComposerDetail = library.getComposerDetail
            let loaded =
                try await Task.detached {
                    try await getComposerDetail(artistId)
                }
                .value
            try Task.checkCancellation()
            guard let loaded else {
                error = String(localized: "Composer detail not found")
                return
            }
            detail = loaded
        }
        catch is CancellationError {}
        catch {
            self.error = error.displayLine
        }
    }
}

private struct ComposerDetailContent: View {
    let detail: BridgeComposerDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        List {
            Section {
                ComposerSummaryRow(summary: detail.composer)
            }
            if !detail.workGroups.isEmpty {
                Section("Works") {
                    ForEach(detail.workGroups, id: \.id) { group in
                        if let parent = group.parent {
                            WorkSummaryButton(summary: parent, openWork: openWork)
                        }
                        ForEach(group.works, id: \.workId) { work in
                            WorkSummaryButton(summary: work, openWork: openWork)
                        }
                    }
                }
            }
            if !detail.unlinkedReleaseRoles.isEmpty {
                Section("Credits") {
                    ForEach(detail.unlinkedReleaseRoles, id: \.releaseId) {
                        role in
                        Button {
                            openAlbum(role.albumId, role.releaseId)
                        } label: {
                            TwoLineRow(
                                title: role.albumTitle,
                                subtitle: role.sourceCredit
                            )
                        }
                    }
                }
            }
            if !detail.unlinkedTrackRoles.isEmpty {
                Section("Recordings") {
                    ForEach(detail.unlinkedTrackRoles, id: \.trackId) { role in
                        TwoLineRow(
                            title: role.trackTitle,
                            subtitle: role.albumTitle
                        )
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }
}

private struct ArtistDetailScreen: View {
    let artistId: String
    let openAlbum: (String) -> Void

    @Environment(Library.self)
    private var library

    @State
    private var detail: BridgeArtistDetail?
    @State
    private var error: String?

    var body: some View {
        Group {
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .padding(32)
            }
            else if let detail {
                ArtistDetailContent(detail: detail, openAlbum: openAlbum)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: artistId) {
            await load()
        }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.artist.name
        }
        if error != nil {
            return String(localized: "Artists")
        }
        return ""
    }

    private func load() async {
        error = nil
        do {
            let getArtistDetail = library.getArtistDetail
            let loaded =
                try await Task.detached {
                    try await getArtistDetail(artistId)
                }
                .value
            try Task.checkCancellation()
            guard let loaded else {
                error = String(localized: "Artist detail not found")
                return
            }
            detail = loaded
        }
        catch is CancellationError {}
        catch {
            self.error = error.displayLine
        }
    }
}

private struct ArtistDetailContent: View {
    let detail: BridgeArtistDetail
    let openAlbum: (String) -> Void

    var body: some View {
        List {
            Section {
                ArtistSummaryRow(summary: detail.artist)
            }
            if !detail.albums.isEmpty {
                Section("Albums") {
                    ForEach(detail.albums) { album in
                        Button {
                            openAlbum(album.id)
                        } label: {
                            HStack(spacing: 12) {
                                ImageView(imageRef: album.cover, pointSize: 48)
                                    .frame(width: 48, height: 48)
                                    .clipShape(RoundedRectangle(cornerRadius: 6))
                                TwoLineRow(
                                    title: album.title,
                                    subtitle: album.year.map(String.init)
                                )
                            }
                        }
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }
}

private struct WorkDetailScreen: View {
    let workId: String
    let openWork: (String) -> Void
    let openAlbum: (BridgeWorkReleaseSummary) -> Void

    @Environment(Library.self)
    private var library

    @State
    private var detail: BridgeWorkDetail?
    @State
    private var error: String?

    var body: some View {
        Group {
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .padding(32)
            }
            else if let detail {
                WorkDetailContent(
                    detail: detail,
                    openWork: openWork,
                    openAlbum: openAlbum
                )
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: workId) {
            await load()
        }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.work.title
        }
        if error != nil {
            return String(localized: "Works")
        }
        return ""
    }

    private func load() async {
        error = nil
        do {
            let getWorkDetail = library.getWorkDetail
            let loaded =
                try await Task.detached {
                    try await getWorkDetail(workId)
                }
                .value
            try Task.checkCancellation()
            guard let loaded else {
                error = String(localized: "Work detail not found")
                return
            }
            detail = loaded
        }
        catch is CancellationError {}
        catch {
            self.error = error.displayLine
        }
    }
}

private struct WorkDetailContent: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (BridgeWorkReleaseSummary) -> Void

    var body: some View {
        List {
            Section {
                WorkSummaryRow(summary: detail.work)
            }
            if !detail.childWorks.isEmpty {
                Section("Works") {
                    ForEach(detail.childWorks, id: \.workId) { work in
                        WorkSummaryButton(summary: work, openWork: openWork)
                    }
                }
            }
            if !detail.releases.isEmpty {
                Section("Releases") {
                    ForEach(detail.releases, id: \.releaseId) { release in
                        Button {
                            openAlbum(release)
                        } label: {
                            HStack(spacing: 12) {
                                ImageView(imageRef: release.cover, pointSize: 42)
                                    .frame(width: 42, height: 42)
                                    .clipShape(RoundedRectangle(cornerRadius: 6))
                                TwoLineRow(
                                    title: release.albumTitle,
                                    subtitle: workReleaseMetadata(release)
                                )
                            }
                        }
                    }
                }
            }
            if !detail.tracks.isEmpty {
                Section("Recordings") {
                    ForEach(detail.tracks, id: \.trackId) { track in
                        TwoLineRow(
                            title: track.trackTitle,
                            subtitle: track.albumTitle
                        )
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    private func workReleaseMetadata(
        _ release: BridgeWorkReleaseSummary
    ) -> String {
        precondition(
            !release.displayName.isEmpty,
            "work release display name is empty for \(release.releaseId)"
        )
        if let format = release.format, !format.isEmpty {
            return "\(release.displayName) \u{00B7} \(format)"
        }
        return release.displayName
    }
}

private struct WorkSummaryButton: View {
    let summary: BridgeWorkSummary
    let openWork: (String) -> Void

    var body: some View {
        Button {
            openWork(summary.workId)
        } label: {
            WorkSummaryRow(summary: summary)
        }
    }
}

private struct WorkSummaryRow: View {
    let summary: BridgeWorkSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.representativeCover, pointSize: 42)
                .frame(width: 42, height: 42)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            TwoLineRow(title: summary.title, subtitle: summary.composerNames)
        }
        .padding(.vertical, 4)
    }
}

private struct TwoLineRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.body)
                .lineLimit(1)
            if let subtitle, !subtitle.isEmpty {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}
