import BaeKit
import SwiftUI

struct LibraryView: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(UiStore.self)
    var uiStore
    @Environment(LibraryBrowseSession.self)
    var session
    @Environment(ConfigStore.self)
    var configStore
    @Environment(LibraryProjectionStore.self)
    private var libraryProjections
    @Environment(ReleaseEditor.self)
    private var releaseEditor
    /// The header's collapse, fed by the panes' `reportsHeaderScroll`.
    @State
    private var headerCollapse = HeaderCollapse()
    /// The album a grid card's menu asked to delete, while its confirmation
    /// shows.
    @State
    private var albumIdPendingDelete: String?

    var body: some View {
        VStack(spacing: 0) {
            libraryHeader
            Group {
                switch uiStore.libraryBrowserMode {
                case .albums:
                    albumContent
                case .composers:
                    composerContent
                case .artists:
                    artistContent
                case .playlists:
                    playlistContent
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .environment(headerCollapse)
        }
        .background(Theme.background)
        .task(id: uiStore.libraryBrowserMode) {
            switch uiStore.libraryBrowserMode {
            case .albums:
                break
            case .composers:
                await session.composers.ensureLoaded()
            case .artists:
                await session.artists.ensureLoaded()
            case .playlists:
                break
            }
        }
        .task { libraryProjections.activatePlaylists() }
        .onChange(of: libraryProjections.composer.value) { _, detail in
            guard let detail,
                case .composer(let artistId, nil) = session.detailSelection,
                artistId == detail.composer.artistId,
                let defaultWorkId = detail.defaultWorkId
            else { return }
            session.selectComposerWork(
                artistId: artistId,
                workId: defaultWorkId
            )
        }
        .onChange(of: libraryProjections.composer.error?.line) { _, line in
            if let line { uiStore.showError(line) }
        }
        .onChange(of: libraryProjections.artist.error?.line) { _, line in
            if let line { uiStore.showError(line) }
        }
        .onChange(of: libraryProjections.work.error?.line) { _, line in
            if let line { uiStore.showError(line) }
        }
        .task(id: uiStore.pendingLibraryNavigation?.seq) {
            guard let request = uiStore.pendingLibraryNavigation else {
                return
            }
            applyLibraryNavigation(request.target)
            uiStore.consumeLibraryNavigation(seq: request.seq)
        }
        .sheet(
            item: Binding(
                get: { uiStore.newPlaylistSheet },
                set: { uiStore.newPlaylistSheet = $0 }
            )
        ) { sheet in
            NewPlaylistSheet(
                // The dismissal frame shows the last real value.
                state: Binding(
                    get: { uiStore.newPlaylistSheet ?? sheet },
                    set: { uiStore.newPlaylistSheet = $0 }
                ),
                onCancel: { uiStore.newPlaylistSheet = nil },
                onCreate: createPlaylist
            )
        }
        // Opening an album's detail clears the multi-selection.
        .onChange(of: uiStore.selectedAlbumId) { _, selected in
            if selected != nil {
                session.albumSelection.clear()
            }
        }
    }
}

extension LibraryView {
    /// Play the grid's albums `albumIds`, in the grid's order, which core
    /// reads.
    private func playAlbums(_ albumIds: [String]) {
        let order = AlbumGridOrder(session: session)
        let library = library
        let playback = playback
        let uiStore = uiStore
        Task {
            do {
                let albums = try await order.albums(albumIds, in: library)
                playback.playReleases(albums.map(\.primaryReleaseId))
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    /// Delete the album `albumId`, closing its detail if it is open; a
    /// failure leaves the album in the grid and says why.
    private func deleteAlbum(_ albumId: String) {
        let releaseEditor = releaseEditor
        let uiStore = uiStore
        Task {
            do {
                try await releaseEditor.deleteAlbum(albumId)
                if uiStore.selectedAlbumId == albumId {
                    uiStore.closeAlbumDetail()
                }
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    /// Make the playlist `sheet` describes and open it; a failure stays in
    /// the sheet, which stays open.
    private func createPlaylist(_ sheet: NewPlaylistSheetState) {
        let library = library
        let session = session
        let uiStore = uiStore
        Task {
            do {
                let playlistId = try await library.createPlaylist(
                    sheet.name,
                    sheet.sharing,
                    sheet.trackIds
                )
                session.selectPlaylist(playlistId)
                uiStore.newPlaylistSheet = nil
            }
            catch {
                uiStore.newPlaylistSheet?.error = DisplayError(error)
            }
        }
    }

    private var queueActions: QueueActions {
        QueueActions(library: library, queue: queue, uiStore: uiStore)
    }

    /// Whether the page spans the window instead of the capped column.
    private var fullWidth: Bool {
        configStore.config.libraryFullWidth
    }

    /// The header above every mode; its heading switches modes.
    private var libraryHeader: some View {
        MacLibraryHeading(
            collapseProgress: headerCollapse.progress,
            fullWidth: fullWidth
        ) {
            switch uiStore.libraryBrowserMode {
            case .albums:
                Toggle(
                    "Group by artist",
                    isOn: Binding(
                        get: { session.albumGrouping.byArtist },
                        set: { session.setGroupAlbumsByArtist($0) }
                    )
                )
                .toggleStyle(.accentCheckbox)
                sortControls(session.albums)
            case .composers:
                sortControls(session.composers)
            case .artists:
                sortControls(session.artists)
            case .playlists:
                if libraryProjections.playlists.value?.canCreate == true {
                    Button("New playlist…") {
                        uiStore.presentNewPlaylist(trackIds: [])
                    }
                }
            }
        }
    }

    private func sortControls<
        Row: Identifiable & Sendable & Equatable,
        Criterion: SortCriterionRepresentable & Codable
    >(
        _ slot: BrowseListSlot<Row, Criterion>
    ) -> some View
    where Row.ID: Sendable {
        MacSortCriteria(
            criteria: Binding(
                get: { slot.sortCriteria },
                set: { slot.setSortCriteria($0) }
            )
        )
    }

    private var albumContent: some View {
        Group {
            if let albumList = session.albums.list {
                if let error = albumList.initialLoadError {
                    Notice(
                        title: String(localized: "Could not read your library"),
                        advice: nil,
                        error: error,
                        look: .centered,
                        action: error.retryAction {
                            Task { await albumList.loadInitial() }
                        }
                    )
                }
                else if albumList.totalCount == 0 {
                    LibraryEmptyState(title: "No albums")
                }
                else {
                    AlbumGridView(
                        list: albumList,
                        sortCriteria: session.albums.sortCriteria,
                        groupByArtist: session.albumGrouping.byArtist,
                        fullWidth: fullWidth,
                        selection: session.albumSelection,
                        onPlay: { albumIds in playAlbums(albumIds) },
                        onAddToQueue: { albumIds in
                            queueActions.addAlbumsToQueue(
                                albumIds,
                                order: AlbumGridOrder(session: session)
                            )
                        },
                        onAddNext: { albumIds in
                            queueActions.addAlbumsNext(
                                albumIds,
                                order: AlbumGridOrder(session: session)
                            )
                        },
                        onDeleteAlbum: { albumId in
                            albumIdPendingDelete = albumId
                        },
                    ) { albumId in
                        AlbumDetailView(albumId: albumId)
                    }
                    .deleteAlbumConfirmation(
                        albumId: $albumIdPendingDelete,
                        onDelete: deleteAlbum
                    )
                }
            }
            else {
                // The first page is a local read; nothing shows until it lands.
                Color.clear
            }
        }
    }

    private var composerContent: some View {
        Group {
            if let composerList = session.composers.list {
                if let error = composerList.initialLoadError {
                    Notice(
                        title: String(localized: "Could not read your library"),
                        advice: nil,
                        error: error,
                        look: .centered,
                        action: error.retryAction {
                            Task { await composerList.loadInitial() }
                        }
                    )
                }
                else if composerList.totalCount == 0 {
                    LibraryEmptyState(
                        title: "No composers",
                        nothingCredited: "No composer credits in your library"
                    )
                }
                else {
                    BrowseSplit {
                        BrowseList(list: composerList) { index in
                            composerRow(at: index, list: composerList)
                        }
                    } detail: {
                        ComposerDetailPane(paneDetail: composerPaneDetail)
                            .detailRead(
                                id: session.detailSelection.composerId,
                                activate: libraryProjections.activateComposer,
                                deactivate: libraryProjections
                                    .deactivateComposer
                            )
                            .detailRead(
                                id: session.detailSelection.workId,
                                activate: libraryProjections.activateWork,
                                deactivate: libraryProjections.deactivateWork
                            )
                    }
                    .libraryContentContainer(fullWidth: fullWidth)
                }
            }
            else {
                // The first page is a local read; nothing shows until it lands.
                Color.clear
            }
        }
    }

    private var artistContent: some View {
        Group {
            if let artistList = session.artists.list {
                if let error = artistList.initialLoadError {
                    Notice(
                        title: String(localized: "Could not read your library"),
                        advice: nil,
                        error: error,
                        look: .centered,
                        action: error.retryAction {
                            Task { await artistList.loadInitial() }
                        }
                    )
                }
                else if artistList.totalCount == 0 {
                    LibraryEmptyState(
                        title: "No artists",
                        nothingCredited: "No artist credits in your library"
                    )
                }
                else {
                    BrowseSplit {
                        BrowseList(list: artistList) { index in
                            artistRow(at: index, list: artistList)
                        }
                    } detail: {
                        ArtistDetailPane(detail: selectedArtistDetail)
                            .detailRead(
                                id: session.selectedArtistId,
                                activate: libraryProjections.activateArtist,
                                deactivate: libraryProjections.deactivateArtist
                            )
                    }
                    .libraryContentContainer(fullWidth: fullWidth)
                }
            }
            else {
                // The first page is a local read; nothing shows until it lands.
                Color.clear
            }
        }
    }

    private var playlistContent: some View {
        PlaylistsContent(
            listing: libraryProjections.playlists.value,
            error: libraryProjections.playlists.error,
            detail: selectedPlaylistDetail,
            fullWidth: fullWidth
        )
        .detailRead(
            id: session.selectedPlaylistId,
            activate: libraryProjections.activatePlaylist,
            deactivate: libraryProjections.deactivatePlaylist
        )
    }

    private var selectedPlaylistDetail: BridgePlaylistDetail? {
        let playlistDetail = libraryProjections.playlist.value
        guard
            playlistDetail?.summary.id == session.selectedPlaylistId
        else { return nil }
        return playlistDetail
    }

    private func applyLibraryNavigation(_ target: LibraryNavigationTarget) {
        switch target {
        case .artist(let artistId):
            session.selectArtist(artistId)
        case .composer(let artistId):
            session.selectComposer(artistId)
        case .work(let workId):
            session.selectWork(workId)
        }
    }

    private func artistRow(at index: Int, list: ArtistList) -> some View {
        browseRow(list.row(at: index), selectedId: session.selectedArtistId) {
            session.selectArtist($0)
        }
    }

    private func composerRow(at index: Int, list: ComposerList) -> some View {
        browseRow(
            list.row(at: index),
            selectedId: session.detailSelection.composerId
        ) {
            session.selectComposer($0)
        }
    }

    /// One row of a master list, drawn from the row the list's query
    /// returned at its position; until that row's page loads it is the
    /// loading row and cannot be selected.
    private func browseRow<Summary: BrowseSummaryDisplay & Identifiable>(
        _ summary: Summary?,
        selectedId: String?,
        select: @escaping (String) -> Void
    ) -> some View where Summary.ID == String {
        MacLibraryRowButton(
            picture: summary?.image,
            title: summary?.name,
            subtitle: summary?.countText,
            selected: summary != nil && selectedId == summary?.id,
            loading: summary == nil,
            action: {
                if let summary {
                    select(summary.id)
                }
            }
        )
    }

    private var composerPaneDetail: ComposerPaneDetail {
        let composerDetail = libraryProjections.composer.value
        let workDetail = libraryProjections.work.value
        switch session.detailSelection {
        case .none:
            return .empty
        case .composer(let artistId, let workId):
            guard composerDetail?.composer.artistId == artistId,
                let composerDetail
            else { return .empty }
            let selectedWork = workId.flatMap { selectedId in
                workDetail?.work.id == selectedId ? workDetail : nil
            }
            let rows = libraryProjections.composerRows
            return .composer(
                composerDetail,
                rows: rows?.artistId == artistId ? rows?.list : nil,
                work: selectedWork
            )
        case .work(let workId):
            guard workDetail?.work.id == workId, let workDetail else {
                return .empty
            }
            return .work(workDetail)
        }
    }

    private var selectedArtistDetail: BridgeArtistDetail? {
        let artistDetail = libraryProjections.artist.value
        guard artistDetail?.artist.artistId == session.selectedArtistId else {
            return nil
        }
        return artistDetail
    }
}
