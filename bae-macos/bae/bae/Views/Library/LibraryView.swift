import BaeKit
import SwiftUI

struct LibraryView: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(Downloads.self)
    var downloads
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(UiStore.self)
    var uiStore
    @Environment(LibraryBrowseSession.self)
    var session
    @Environment(ConfigStore.self)
    var configStore

    /// Detail payloads derived from `session.detailSelection` /
    /// `session.selectedArtistId`. View-local by design (see
    /// `LibraryBrowseSession`'s doc comment): a remount re-fetches them cheaply
    /// rather than keeping them warm, while the selections themselves persist.
    @State
    private var composerPaneDetail: ComposerPaneDetail = .empty
    @State
    private var artistDetail: BridgeArtistDetail?
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
        // The page sits on a faint radial lift toward the top-trailing corner
        // rather than a flat fill, so the header band reads as its own region.
        // One surface step above the base — any lighter reads as a spotlight.
        .background(
            RadialGradient(
                colors: [Theme.surface, Theme.background],
                center: UnitPoint(x: 0.8, y: -0.1),
                startRadius: 0,
                endRadius: 900
            )
        )
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
        .task(id: session.detailSelection.composerId) {
            await loadComposerDetail()
        }
        .task(id: session.detailSelection) {
            await loadWorkDetail()
        }
        .task(id: session.selectedArtistId) {
            await loadArtistDetail()
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
        // The album-list invalidation is coarse (no per-id removal event), so
        // after a shape change prune any selected album that no longer resolves —
        // e.g. one deleted on another device. The selection is small; one
        // authoritative index lookup per id, cancellable between ids.
        .task(id: session.albums.list?.loadEpoch) {
            await pruneDeletedSelection()
        }
        // Mirror the library-wide album count into the store: the menu bar's
        // Shuffle Library item (MainAppMenuCommands) disables at zero, and the
        // menu has no album list of its own to ask. `initial: true` publishes
        // a count that was already loaded when this view (re)mounts.
        .onChange(of: session.albums.list?.totalCount, initial: true) {
            _,
            count in
            if let count {
                libraryStore.setAlbumTotal(count)
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

    /// Drop any selected album that no longer resolves under the current sort —
    /// the authoritative `getAlbumIndex` returning nil means it was deleted. Runs
    /// off the album-list epoch; checks cancellation between ids so a newer epoch
    /// supersedes it cleanly.
    private func pruneDeletedSelection() async {
        guard !session.albumSelection.isEmpty else {
            return
        }
        var missing: [String] = []
        for id in session.albumSelection.selectedIds {
            if Task.isCancelled {
                return
            }
            do {
                if try await library.getAlbumIndex(
                    session.albums.sortCriteria,
                    id
                )
                    == nil
                {
                    missing.append(id)
                }
            }
            catch {
                // A lookup failure isn't evidence the album is gone; leave the
                // selection untouched rather than prune on a transient error.
                return
            }
        }
        if !missing.isEmpty {
            session.albumSelection.remove(missing)
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
                        onPin: { albumIds in
                            let releaseIds = primaryReleaseIds(for: albumIds)
                            Task { try await downloads.queuePins(releaseIds) }
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
                        ComposerDetailPane(paneDetail: $composerPaneDetail)
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
                        ArtistDetailPane(detail: artistDetail)
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
            artistDetail = nil
        case .composer(let artistId):
            session.selectComposer(artistId)
            composerPaneDetail = .empty
        case .work(let workId):
            session.selectWork(workId)
            composerPaneDetail = .empty
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
                artistDetail = nil
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
                composerPaneDetail = .empty
            }
        )
    }

    private func loadComposerDetail() async {
        guard let selectedComposerId = session.detailSelection.composerId
        else {
            return
        }
        do {
            let getComposerDetail = library.getComposerDetail
            let detail = try await getComposerDetail(selectedComposerId)
            guard !Task.isCancelled else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Composer detail not found")
                    )
                )
                return
            }
            composerPaneDetail = .composer(detail, work: nil)
            if case .composer(let artistId, nil) = session.detailSelection,
                artistId == selectedComposerId,
                let defaultWorkId = detail.defaultWorkId
            {
                session.selectComposerWork(
                    artistId: selectedComposerId,
                    workId: defaultWorkId
                )
            }
        }
        catch {
            uiStore.showError(error)
        }
    }

    private func loadArtistDetail() async {
        guard let requestedArtistId = session.selectedArtistId else {
            return
        }
        do {
            let getArtistDetail = library.getArtistDetail
            let detail = try await getArtistDetail(requestedArtistId)
            guard !Task.isCancelled else {
                return
            }
            guard session.selectedArtistId == requestedArtistId else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Artist detail not found")
                    )
                )
                return
            }
            artistDetail = detail
        }
        catch {
            uiStore.showError(error)
        }
    }

    private func loadWorkDetail() async {
        let selectedDetail = session.detailSelection
        guard let selectedWorkId = selectedDetail.workId else {
            return
        }
        do {
            let getWorkDetail = library.getWorkDetail
            let detail = try await getWorkDetail(selectedWorkId)
            guard !Task.isCancelled else {
                return
            }
            guard session.detailSelection == selectedDetail else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Work detail not found")
                    )
                )
                return
            }
            switch selectedDetail {
            case .composer(let artistId, .some):
                guard
                    case .composer(let composerDetail, _) = composerPaneDetail,
                    composerDetail.composer.artistId == artistId
                else {
                    uiStore.showError(
                        DisplayError(
                            line: String(localized: "Composer detail not found")
                        )
                    )
                    return
                }
                composerPaneDetail = .composer(composerDetail, work: detail)
            case .work:
                composerPaneDetail = .work(detail)
            case .none, .composer(_, .none):
                return
            }
        }
        catch {
            uiStore.showError(error)
        }
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
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(library)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore)
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
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(library)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore)
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
            .environment(ImageStore.stub)
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(library)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore)
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
            .environment(ImageStore.stub)
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(library)
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
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(library)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore)
            .frame(width: 1100, height: 700)
            .windowBackground()
    }
#endif
