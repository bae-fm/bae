import BaeKit
import SwiftUI

struct LibraryView: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(UiStore.self)
    var uiStore
    @Environment(LibraryBrowseSession.self)
    var session
    @Environment(ConfigStore.self)
    var configStore
    @Environment(LibraryProjectionStore.self)
    private var libraryProjections
    /// Collapse kinematics for the header, fed by the panes' scroll reports
    /// (`reportsHeaderScroll`). The header scrubs between its full and
    /// compact metrics off `headerCollapse.progress`, reclaiming the
    /// vertical room the full-size heading occupies at rest.
    @State
    private var headerCollapse = HeaderCollapse()

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
            }
        }
        .onChange(of: session.detailSelection.composerId, initial: true) {
            oldId,
            newId in
            if let oldId { libraryProjections.deactivateComposer(oldId) }
            if let newId { libraryProjections.activateComposer(newId) }
        }
        .onChange(of: session.detailSelection.workId, initial: true) {
            oldId,
            newId in
            if let oldId { libraryProjections.deactivateWork(oldId) }
            if let newId { libraryProjections.activateWork(newId) }
        }
        .onChange(of: session.selectedArtistId, initial: true) { oldId, newId in
            if let oldId { libraryProjections.deactivateArtist(oldId) }
            if let newId { libraryProjections.activateArtist(newId) }
        }
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
        // Opening a detail expansion (plain click, search, external navigation)
        // clears the multi-selection — the same semantics as a plain grid click.
        .onChange(of: uiStore.selectedAlbumId) { _, selected in
            if selected != nil {
                session.albumSelection.clear()
            }
        }
    }
}

extension LibraryView {
    private var queueActions: QueueActions {
        QueueActions(library: library, queue: queue, uiStore: uiStore)
    }

    /// The primary release id of each album, in the album order given. The grid's
    /// bulk Play and Pin act on releases; a selected album is loaded (its summary
    /// is interned), so the lookup resolves.
    private func primaryReleaseIds(for albumIds: [String]) -> [String] {
        albumIds.compactMap {
            libraryStore.albumSummaries[$0]?.primaryReleaseId
        }
    }

    /// Whether the page spans the window instead of centering in the shared
    /// capped column — the user's `libraryFullWidth` setting.
    private var fullWidth: Bool {
        configStore.config.libraryFullWidth
    }

    /// Pinned above the content, fixed across mode switches. The heading *is*
    /// the mode switcher; the trailing controls are mode-specific.
    private var libraryHeader: some View {
        LibraryHeader(
            collapseProgress: headerCollapse.progress,
            fullWidth: fullWidth
        ) {
            switch uiStore.libraryBrowserMode {
            case .albums:
                sortControls(session.albums)
            case .composers:
                sortControls(session.composers)
            case .artists:
                sortControls(session.artists)
            }
        }
    }

    private func sortControls<
        Row: Identifiable & Sendable,
        Criterion: SortCriterionRepresentable
    >(
        _ slot: BrowseListSlot<Row, Criterion>
    ) -> some View
    where Row.ID: Sendable, Criterion.Field: SortCriterionFieldCodable {
        SortCriteriaRow(
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
                    LoadFailureView(line: error.line) {
                        Task { await albumList.loadInitial() }
                    }
                }
                else if albumList.totalCount == 0 {
                    ContentUnavailableView {
                        Text("No albums")
                    } description: {
                        Text("Import some music to get started")
                    }
                }
                else {
                    AlbumGridView(
                        list: albumList,
                        sortCriteria: session.albums.sortCriteria,
                        fullWidth: fullWidth,
                        selection: session.albumSelection,
                        onPlay: { albumIds in
                            playback.playReleases(
                                primaryReleaseIds(for: albumIds)
                            )
                        },
                        onAddToQueue: { albumIds in
                            queueActions.addToQueue(albumIds)
                        },
                        onAddNext: { albumIds in
                            queueActions.addNext(albumIds)
                        },
                    ) { albumId in
                        AlbumDetailView(albumId: albumId)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private var composerContent: some View {
        Group {
            if let composerList = session.composers.list {
                if let error = composerList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await composerList.loadInitial() }
                    }
                }
                else if composerList.totalCount == 0 {
                    ContentUnavailableView {
                        Text("No composers")
                    } description: {
                        Text("Import some music to get started")
                    }
                }
                else {
                    HSplitView {
                        BrowseList(list: composerList) { index in
                            composerRow(at: index, list: composerList)
                        }
                        .frame(minWidth: 260, idealWidth: 320)
                        ComposerDetailPane(paneDetail: composerPaneDetail)
                            .frame(minWidth: 420)
                    }
                    .libraryContentContainer(fullWidth: fullWidth)
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private var artistContent: some View {
        Group {
            if let artistList = session.artists.list {
                if let error = artistList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await artistList.loadInitial() }
                    }
                }
                else if artistList.totalCount == 0 {
                    ContentUnavailableView {
                        Text("No artists")
                    } description: {
                        Text("Import some music to get started")
                    }
                }
                else {
                    HSplitView {
                        BrowseList(list: artistList) { index in
                            artistRow(at: index, list: artistList)
                        }
                        .frame(minWidth: 260, idealWidth: 320)
                        ArtistDetailPane(detail: selectedArtistDetail)
                            .frame(minWidth: 420)
                    }
                    .libraryContentContainer(fullWidth: fullWidth)
                }
            }
            else {
                ProgressView()
            }
        }
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
        let id = list.idAt(index)
        return BrowseListRow(
            id: id,
            isSelected: id != nil && session.selectedArtistId == id,
            summaries: \.artistSummaries,
            select: { id in
                session.selectArtist(id)
            }
        )
    }

    private func composerRow(at index: Int, list: ComposerList) -> some View {
        let id = list.idAt(index)
        return BrowseListRow(
            id: id,
            isSelected: id != nil && session.detailSelection.composerId == id,
            summaries: \.composerSummaries,
            select: { id in
                session.selectComposer(id)
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
            return .composer(composerDetail, work: selectedWork)
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

#if DEBUG
    #Preview("Albums \u{2014} Empty") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewEmptyBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        return LibraryView()
            .environment(Playback.stub())
            .environment(Queue.stub())
            .environment(Downloads.stub())
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore())
            .frame(width: 1100, height: 700)
            .windowBackground()
    }

    #Preview("Composers \u{2014} Empty") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.composers)
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewEmptyBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        return LibraryView()
            .environment(Playback.stub())
            .environment(Queue.stub())
            .environment(Downloads.stub())
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore())
            .frame(width: 1100, height: 700)
            .windowBackground()
    }

    #Preview("Composers \u{2014} Detail") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.composers)
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewComposerBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        return LibraryView()
            .environment(ImageStore.stub())
            .environment(Playback.stub())
            .environment(Queue.stub())
            .environment(Downloads.stub())
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore())
            .frame(width: 1200, height: 760)
            .windowBackground()
    }

    #Preview("Albums \u{2014} Grid") {
        PreviewScenes.libraryGrid()
            .frame(width: 1500, height: 700)
    }

    /// The same populated grid with the width cap lifted
    /// (`libraryFullWidth`): header and grid span the window edge to edge.
    #Preview("Albums \u{2014} Grid, full width") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewGridBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        return LibraryView()
            .environment(ImageStore.stub())
            .environment(Playback.stub())
            .environment(Queue.stub())
            .environment(Downloads.stub())
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.makeConfigStore(libraryFullWidth: true))
            .frame(width: 1500, height: 700)
            .windowBackground()
    }

    #Preview("Artists \u{2014} Empty") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.artists)
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewEmptyBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        return LibraryView()
            .environment(Playback.stub())
            .environment(Queue.stub())
            .environment(Downloads.stub())
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore())
            .frame(width: 1100, height: 700)
            .windowBackground()
    }
#endif
