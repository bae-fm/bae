import BaeKit
import SwiftUI

private let listLoadBatchSize = 50

private enum ComposerPaneSelection: Equatable {
    case none
    case composer(artistId: String, workId: String?)
    case work(workId: String)

    var composerId: String? {
        if case .composer(let artistId, _) = self {
            return artistId
        }
        return nil
    }

    var workId: String? {
        switch self {
        case .none:
            return nil
        case .composer(_, let workId):
            return workId
        case .work(let workId):
            return workId
        }
    }
}

private enum ComposerPaneDetail {
    case empty
    case composer(BridgeComposerDetail, work: BridgeWorkDetail?)
    case work(BridgeWorkDetail)
}

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
    @Environment(ProjectionRegistry.self)
    var projectionRegistry

    @State
    private var albumList: AlbumList?
    /// Album-grid multi-selection, a browsing-session concern owned here as local
    /// state (the Storage Manager precedent) and passed to the grid.
    @State
    private var albumSelection = AlbumGridSelection()
    @State
    private var composerList: ComposerList?
    @State
    private var albumListRegistration: ProjectionRegistration?
    @State
    private var composerListRegistration: ProjectionRegistration?
    @State
    private var sortCriteria: [BridgeSortCriterion] = Self.loadSortCriteria()
    @State
    private var composerSortCriteria: [BridgeComposerSortCriterion] =
        Self.loadComposerSortCriteria()
    @State
    private var composerPaneDetail: ComposerPaneDetail = .empty
    @State
    private var detailSelection: ComposerPaneSelection = .none
    @State
    private var artistList: ArtistList?
    @State
    private var artistListRegistration: ProjectionRegistration?
    @State
    private var artistSortCriteria: [BridgeArtistSortCriterion] =
        Self.loadArtistSortCriteria()
    @State
    private var selectedArtistId: String?
    @State
    private var artistDetail: BridgeArtistDetail?

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
        }
        .background(Theme.background)
        .task(id: sortCriteria) {
            Self.saveSortCriteria(sortCriteria)
            await reloadAlbumList(sort: sortCriteria)
        }
        .task(id: uiStore.libraryBrowserMode) {
            switch uiStore.libraryBrowserMode {
            case .albums:
                break
            case .composers:
                await ensureComposerListLoaded()
            case .artists:
                await ensureArtistListLoaded()
            }
        }
        .task(id: detailSelection.composerId) {
            await loadComposerDetail()
        }
        .task(id: detailSelection) {
            await loadWorkDetail()
        }
        .task(id: composerSortCriteria) {
            Self.saveComposerSortCriteria(composerSortCriteria)
            if uiStore.libraryBrowserMode == .composers {
                await reloadComposerList(sort: composerSortCriteria)
            }
        }
        .task(id: artistSortCriteria) {
            Self.saveArtistSortCriteria(artistSortCriteria)
            if uiStore.libraryBrowserMode == .artists {
                await reloadArtistList(sort: artistSortCriteria)
            }
        }
        .task(id: selectedArtistId) {
            await loadArtistDetail()
        }
        .task(id: uiStore.libraryNavigationRequest?.seq) {
            guard let request = uiStore.libraryNavigationRequest else {
                return
            }
            applyLibraryNavigation(request.target)
        }
        // Opening a detail expansion (plain click, search, external navigation)
        // clears the multi-selection — the same semantics as a plain grid click.
        .onChange(of: uiStore.selectedAlbumId) { _, selected in
            if selected != nil {
                albumSelection.clear()
            }
        }
        // The album-list invalidation is coarse (no per-id removal event), so
        // after a shape change prune any selected album that no longer resolves —
        // e.g. one deleted on another device. The selection is small; one
        // authoritative index lookup per id, cancellable between ids.
        .task(id: albumList?.loadEpoch) {
            await pruneDeletedSelection()
        }
        // Mirror the library-wide album count into the store: the menu bar's
        // Shuffle Library item (MainAppMenuCommands) disables at zero, and the
        // menu has no album list of its own to ask. `initial: true` publishes
        // a count that was already loaded when this view (re)mounts.
        .onChange(of: albumList?.totalCount, initial: true) { _, count in
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
        guard !albumSelection.isEmpty else {
            return
        }
        var missing: [String] = []
        for id in albumSelection.selectedIds {
            if Task.isCancelled {
                return
            }
            do {
                if try await library.getAlbumIndex(sortCriteria, id) == nil {
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
            albumSelection.remove(missing)
        }
    }

    /// Pinned above the content, fixed across mode switches. The heading *is*
    /// the mode switcher; the trailing controls are mode-specific.
    private var libraryHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            LibraryModeHeading()
            Spacer()
            switch uiStore.libraryBrowserMode {
            case .albums:
                albumSortControls
            case .composers:
                composerSortControls
            case .artists:
                artistSortControls
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 40)
        .padding(.bottom, 20)
    }

    private var albumSortControls: some View {
        SortCriteriaRow(criteria: $sortCriteria)
    }

    private var albumContent: some View {
        Group {
            if let albumList {
                if let error = albumList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await albumList.loadInitial() }
                    }
                }
                else if albumList.totalCount == 0 {
                    ContentUnavailableView(
                        "No albums",
                        systemImage: "square.stack",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    AlbumGridView(
                        list: albumList,
                        sortCriteria: sortCriteria,
                        selection: albumSelection,
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
                            Task { await downloads.queuePins(releaseIds) }
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
            if let composerList {
                if let error = composerList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await composerList.loadInitial() }
                    }
                }
                else if composerList.totalCount == 0 {
                    ContentUnavailableView(
                        "No composers",
                        systemImage: "person.wave.2",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    HSplitView {
                        composerListView(composerList)
                            .frame(minWidth: 260, idealWidth: 320)
                        composerDetailView
                            .frame(minWidth: 420)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private var artistContent: some View {
        Group {
            if let artistList {
                if let error = artistList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await artistList.loadInitial() }
                    }
                }
                else if artistList.totalCount == 0 {
                    ContentUnavailableView(
                        "No artists",
                        systemImage: "music.mic",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    HSplitView {
                        artistListView(artistList)
                            .frame(minWidth: 260, idealWidth: 320)
                        artistDetailView
                            .frame(minWidth: 420)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private func applyLibraryNavigation(_ target: LibraryNavigationTarget) {
        switch target {
        case .composer(let artistId):
            detailSelection = .composer(artistId: artistId, workId: nil)
            composerPaneDetail = .empty
        case .work(let workId):
            detailSelection = .work(workId: workId)
            composerPaneDetail = .empty
        }
    }

    private func composerListView(_ list: ComposerList) -> some View {
        List {
            ForEach(0..<list.totalCount, id: \.self) { index in
                composerRow(at: index, list: list)
                    .task(
                        id: RowLoadID(
                            epoch: list.loadEpoch,
                            index: index
                        )
                    ) {
                        let first = max(
                            0,
                            index - listLoadBatchSize / 2
                        )
                        let end = min(
                            first + listLoadBatchSize,
                            list.totalCount
                        )
                        await list.loadRange(
                            offset: first,
                            limit: end - first
                        )
                    }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
    }

    private func artistListView(_ list: ArtistList) -> some View {
        List {
            ForEach(0..<list.totalCount, id: \.self) { index in
                artistRow(at: index, list: list)
                    .task(
                        id: RowLoadID(
                            epoch: list.loadEpoch,
                            index: index
                        )
                    ) {
                        let first = max(0, index - listLoadBatchSize / 2)
                        let end = min(
                            first + listLoadBatchSize,
                            list.totalCount
                        )
                        await list.loadRange(
                            offset: first,
                            limit: end - first
                        )
                    }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
    }

    private func artistRow(at index: Int, list: ArtistList) -> some View {
        let id = list.idAt(index)
        return ArtistListRow(
            id: id,
            isSelected: id != nil && selectedArtistId == id,
            select: { id in
                selectedArtistId = id
                artistDetail = nil
            }
        )
    }

    private var composerSortControls: some View {
        SortCriteriaRow(criteria: $composerSortCriteria)
    }

    private var artistSortControls: some View {
        SortCriteriaRow(criteria: $artistSortCriteria)
    }

    private func composerRow(at index: Int, list: ComposerList) -> some View {
        let id = list.idAt(index)
        return ComposerListRow(
            id: id,
            isSelected: id != nil && detailSelection.composerId == id,
            select: { id in
                detailSelection = .composer(artistId: id, workId: nil)
                composerPaneDetail = .empty
            }
        )
    }

    private var composerDetailView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if case .composer(let composerDetail, let loadedWorkDetail) =
                    composerPaneDetail
                {
                    ComposerDetailHeader(summary: composerDetail.composer)
                    if !composerDetail.workGroups.isEmpty {
                        SectionHeader(title: String(localized: "Works"))
                        ForEach(composerDetail.workGroups) { group in
                            ComposerWorkGroupView(group: group) { workId in
                                if let artistId = detailSelection.composerId {
                                    detailSelection = .composer(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    composerPaneDetail = .composer(
                                        composerDetail,
                                        work: nil
                                    )
                                }
                            }
                        }
                    }
                    if !composerDetail.unlinkedReleaseRoles.isEmpty {
                        SectionHeader(title: String(localized: "Credits"))
                        ForEach(
                            composerDetail.unlinkedReleaseRoles,
                            id: \.releaseId
                        ) { role in
                            CreditRow(
                                title: role.albumTitle,
                                subtitle: role.sourceCredit
                            )
                        }
                    }
                    if !composerDetail.unlinkedTrackRoles.isEmpty {
                        ForEach(
                            composerDetail.unlinkedTrackRoles,
                            id: \.trackId
                        ) { role in
                            CreditRow(
                                title: role.trackTitle,
                                subtitle: role.albumTitle
                            )
                        }
                    }
                    if let loadedWorkDetail {
                        Divider()
                        WorkDetailView(
                            detail: loadedWorkDetail,
                            openWork: { workId in
                                if case .composer(let artistId, _) =
                                    detailSelection
                                {
                                    detailSelection = .composer(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    composerPaneDetail = .composer(
                                        composerDetail,
                                        work: nil
                                    )
                                }
                            },
                            openAlbum: { albumId, releaseId in
                                uiStore.navigateToAlbum(
                                    albumId,
                                    releaseId: releaseId
                                )
                            }
                        )
                    }
                }
                if case .work(let loadedWorkDetail) = composerPaneDetail {
                    WorkDetailView(
                        detail: loadedWorkDetail,
                        openWork: { workId in
                            detailSelection = .work(workId: workId)
                            composerPaneDetail = .empty
                        },
                        openAlbum: { albumId, releaseId in
                            uiStore.navigateToAlbum(
                                albumId,
                                releaseId: releaseId
                            )
                        }
                    )
                }
                if detailSelection == .none {
                    ContentUnavailableView(
                        "Composers",
                        systemImage: "person.wave.2"
                    )
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Theme.surface)
    }

    private var artistDetailView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let artistDetail {
                    ArtistDetailHeader(summary: artistDetail.artist)
                    LazyVGrid(
                        columns: [
                            GridItem(.adaptive(minimum: 140), spacing: 12)
                        ],
                        alignment: .leading,
                        spacing: 12
                    ) {
                        ForEach(artistDetail.albums) { album in
                            ArtistAlbumCard(album: album) {
                                uiStore.navigateToAlbum(album.id)
                            }
                        }
                    }
                }
                else if selectedArtistId == nil {
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
        .background(Theme.surface)
    }

    private func ensureComposerListLoaded() async {
        guard composerList == nil else {
            return
        }
        await reloadComposerList(sort: composerSortCriteria)
    }

    private func ensureArtistListLoaded() async {
        guard artistList == nil else {
            return
        }
        await reloadArtistList(sort: artistSortCriteria)
    }

    private func loadComposerDetail() async {
        guard let selectedComposerId = detailSelection.composerId else {
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
            if case .composer(let artistId, nil) = detailSelection,
                artistId == selectedComposerId,
                let defaultWorkId = detail.defaultWorkId
            {
                detailSelection = .composer(
                    artistId: selectedComposerId,
                    workId: defaultWorkId
                )
            }
        }
        catch {
            uiStore.showError(DisplayError(line: error.localizedDescription))
        }
    }

    private func loadArtistDetail() async {
        guard let requestedArtistId = selectedArtistId else {
            return
        }
        do {
            let getArtistDetail = library.getArtistDetail
            let detail = try await getArtistDetail(requestedArtistId)
            guard !Task.isCancelled else {
                return
            }
            guard selectedArtistId == requestedArtistId else {
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
            uiStore.showError(DisplayError(line: error.localizedDescription))
        }
    }

    private func loadWorkDetail() async {
        let selectedDetail = detailSelection
        guard let selectedWorkId = selectedDetail.workId else {
            return
        }
        do {
            let getWorkDetail = library.getWorkDetail
            let detail = try await getWorkDetail(selectedWorkId)
            guard !Task.isCancelled else {
                return
            }
            guard detailSelection == selectedDetail else {
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
            uiStore.showError(DisplayError(line: error.localizedDescription))
        }
    }

    private func makeAlbumList(sort: [BridgeSortCriterion]) -> AlbumList {
        AlbumList(
            pageSource: LibraryAlbumPageSource(
                library: library,
                sort: sort
            ),
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row)
                }
            },
            onError: { [uiStore] error in
                uiStore.showError(error)
            },
        )
    }

    private func makeComposerList(
        sort: [BridgeComposerSortCriterion]
    ) -> ComposerList {
        ComposerList(
            pageSource: LibraryComposerPageSource(
                library: library,
                sort: sort
            ),
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internComposerSummary(row)
                }
            },
            onError: { [uiStore] error in
                uiStore.showError(error)
            },
        )
    }

    private func makeArtistList(
        sort: [BridgeArtistSortCriterion]
    ) -> ArtistList {
        ArtistList(
            pageSource: LibraryArtistPageSource(
                library: library,
                sort: sort
            ),
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internArtistSummary(row)
                }
            },
            onError: { [uiStore] error in
                uiStore.showError(error)
            },
        )
    }

    private func reloadComposerList(sort: [BridgeComposerSortCriterion]) async {
        let newList = makeComposerList(sort: sort)
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        composerListRegistration = projectionRegistry.registerList(
            newList,
            domain: .composerList
        )
        composerList = newList
    }

    private func reloadArtistList(sort: [BridgeArtistSortCriterion]) async {
        let newList = makeArtistList(sort: sort)
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        artistListRegistration = projectionRegistry.registerList(
            newList,
            domain: .artistList
        )
        artistList = newList
    }

    private func reloadAlbumList(sort: [BridgeSortCriterion]) async {
        let newList = makeAlbumList(sort: sort)
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        albumListRegistration = projectionRegistry.registerList(
            newList,
            domain: .albumList
        )
        albumList = newList
    }

    private static let sortCriteriaKey = "librarySortCriteria"

    private static func loadSortCriteria() -> [BridgeSortCriterion] {
        guard let data = UserDefaults.standard.data(forKey: sortCriteriaKey),
            let criteria = [BridgeSortCriterion].fromJSON(data),
            !criteria.isEmpty
        else {
            return [
                BridgeSortCriterion(field: .dateAdded, direction: .descending)
            ]
        }
        return criteria
    }

    private static func saveSortCriteria(_ criteria: [BridgeSortCriterion]) {
        if let data = criteria.toJSON() {
            UserDefaults.standard.set(data, forKey: sortCriteriaKey)
        }
    }

    private static let composerSortCriteriaKey = "libraryComposerSortCriteria"

    private static func loadComposerSortCriteria()
        -> [BridgeComposerSortCriterion]
    {
        guard
            let data = UserDefaults.standard.data(
                forKey: composerSortCriteriaKey
            ),
            let criteria = [BridgeComposerSortCriterion].fromJSON(data),
            !criteria.isEmpty
        else {
            return [
                BridgeComposerSortCriterion(
                    field: .name,
                    direction: .ascending
                )
            ]
        }
        return criteria
    }

    private static func saveComposerSortCriteria(
        _ criteria: [BridgeComposerSortCriterion]
    ) {
        if let data = criteria.toJSON() {
            UserDefaults.standard.set(data, forKey: composerSortCriteriaKey)
        }
    }

    private static let artistSortCriteriaKey = "libraryArtistSortCriteria"

    private static func loadArtistSortCriteria() -> [BridgeArtistSortCriterion]
    {
        guard
            let data = UserDefaults.standard.data(
                forKey: artistSortCriteriaKey
            ),
            let criteria = [BridgeArtistSortCriterion].fromJSON(data),
            !criteria.isEmpty
        else {
            return [
                BridgeArtistSortCriterion(field: .name, direction: .ascending)
            ]
        }
        return criteria
    }

    private static func saveArtistSortCriteria(
        _ criteria: [BridgeArtistSortCriterion]
    ) {
        if let data = criteria.toJSON() {
            UserDefaults.standard.set(data, forKey: artistSortCriteriaKey)
        }
    }

}

extension BridgeComposerWorkGroup: Identifiable {}

private struct ComposerListRow: View {
    @Environment(LibraryStore.self)
    private var libraryStore

    let id: String?
    let isSelected: Bool
    let select: (String) -> Void

    var body: some View {
        let summary = id.flatMap { libraryStore.composerSummaries[$0] }
        Button(action: {
            guard let id else {
                return
            }
            select(id)
        }) {
            ZStack(alignment: .leading) {
                SummaryRowPlaceholder()
                    .opacity(summary == nil ? 1 : 0)
                    .allowsHitTesting(summary == nil)
                ComposerSummaryRow(summary: summary, isSelected: isSelected)
                    .opacity(summary == nil ? 0 : 1)
                    .allowsHitTesting(summary != nil)
            }
        }
        .buttonStyle(.plain)
        .disabled(summary == nil)
    }
}

private struct ComposerSummaryRow: View {
    let summary: BridgeComposerSummary?
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary?.image, pointSize: 36)
                .frame(width: 36, height: 36)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 3) {
                OptionalLineText(
                    text: summary?.name,
                    font: .body,
                    foreground: .primary
                )
                OptionalLineText(
                    text: workCountText,
                    font: .caption,
                    foreground: .secondary
                )
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(isSelected ? Color.accentColor.opacity(0.18) : .clear)
        )
    }

    private var workCountText: String? {
        guard let summary else {
            return nil
        }
        return "\(summary.workCount) \(String(localized: "Works"))"
    }
}

private struct ArtistListRow: View {
    @Environment(LibraryStore.self)
    private var libraryStore

    let id: String?
    let isSelected: Bool
    let select: (String) -> Void

    var body: some View {
        let summary = id.flatMap { libraryStore.artistSummaries[$0] }
        Button(action: {
            guard let id else {
                return
            }
            select(id)
        }) {
            ZStack(alignment: .leading) {
                SummaryRowPlaceholder()
                    .opacity(summary == nil ? 1 : 0)
                    .allowsHitTesting(summary == nil)
                ArtistSummaryRow(summary: summary, isSelected: isSelected)
                    .opacity(summary == nil ? 0 : 1)
                    .allowsHitTesting(summary != nil)
            }
        }
        .buttonStyle(.plain)
        .disabled(summary == nil)
    }
}

private struct ArtistSummaryRow: View {
    let summary: BridgeArtistSummary?
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary?.image, pointSize: 36)
                .frame(width: 36, height: 36)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 3) {
                OptionalLineText(
                    text: summary?.name,
                    font: .body,
                    foreground: .primary
                )
                OptionalLineText(
                    text: albumCountText,
                    font: .caption,
                    foreground: .secondary
                )
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(isSelected ? Color.accentColor.opacity(0.18) : .clear)
        )
    }

    private var albumCountText: String? {
        guard let summary else {
            return nil
        }
        return "\(summary.albumCount) \(String(localized: "Albums"))"
    }
}

private struct OptionalLineText: View {
    let text: String?
    let font: Font
    let foreground: StableOptionalTextForeground

    var body: some View {
        StableOptionalText(
            text: text,
            font: font,
            foreground: foreground,
            lineHeight: lineHeight,
            lineLimit: 1
        )
    }

    private var lineHeight: CGFloat {
        switch font {
        case .caption:
            12
        default:
            16
        }
    }
}

private struct SummaryRowPlaceholder: View {
    var body: some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 6)
                .fill(.secondary.opacity(0.15))
                .frame(width: 36, height: 36)
            VStack(alignment: .leading, spacing: 6) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 140, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 80, height: 10)
            }
        }
        .padding(.vertical, 4)
    }
}

private struct ComposerDetailHeader: View {
    let summary: BridgeComposerSummary

    var body: some View {
        HStack(spacing: 16) {
            ImageView(imageRef: summary.image, pointSize: 72)
                .frame(width: 72, height: 72)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 6) {
                Text(summary.name)
                    .font(.title.bold())
                    .lineLimit(2)
                Text("\(summary.workCount) \(String(localized: "Works"))")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct ArtistDetailHeader: View {
    let summary: BridgeArtistSummary

    var body: some View {
        HStack(spacing: 16) {
            ImageView(imageRef: summary.image, pointSize: 72)
                .frame(width: 72, height: 72)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 6) {
                Text(summary.name)
                    .font(.title.bold())
                    .lineLimit(2)
                Text("\(summary.albumCount) \(String(localized: "Albums"))")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct ArtistAlbumCard: View {
    let album: BridgeAlbum
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(imageRef: album.cover, pointSize: 140)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(album.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                StableOptionalText(
                    text: album.year.map(String.init),
                    font: .caption,
                    foreground: .secondary,
                    lineHeight: 12,
                    lineLimit: 1
                )
            }
        }
        .buttonStyle(.plain)
    }
}

private struct WorkSummaryRow: View {
    let summary: BridgeWorkSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.representativeCover, pointSize: 42)
                .frame(width: 42, height: 42)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 3) {
                Text(summary.title)
                    .font(.body)
                    .lineLimit(1)
                OptionalLineText(
                    text: summary.composerNames,
                    font: .caption,
                    foreground: .secondary
                )
            }
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct ComposerWorkGroupView: View {
    let group: BridgeComposerWorkGroup
    let openWork: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(parentRows, id: \.workId) { parent in
                Button(action: { openWork(parent.workId) }) {
                    WorkSummaryRow(summary: parent)
                }
                .buttonStyle(.plain)
            }
            VStack(alignment: .leading, spacing: 6) {
                ForEach(group.works, id: \.workId) { work in
                    Button(action: { openWork(work.workId) }) {
                        WorkSummaryRow(summary: work)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.leading, group.parent == nil ? 0 : 18)
        }
    }

    private var parentRows: [BridgeWorkSummary] {
        guard let parent = group.parent else {
            return []
        }
        return [parent]
    }
}

private struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.headline)
            .padding(.top, 4)
    }
}

private struct CreditRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.body)
                .lineLimit(1)
            StableOptionalText(
                text: subtitle,
                font: .caption,
                foreground: .secondary,
                lineHeight: 12,
                lineLimit: 1
            )
        }
        .padding(.vertical, 4)
    }
}

private struct WorkDetailView: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(detail.work.title)
                .font(.title2.bold())
                .lineLimit(2)
            if !detail.childWorks.isEmpty {
                SectionHeader(title: String(localized: "Works"))
                ForEach(detail.childWorks, id: \.workId) { work in
                    Button(action: { openWork(work.workId) }) {
                        WorkSummaryRow(summary: work)
                    }
                    .buttonStyle(.plain)
                }
            }
            if !detail.releases.isEmpty {
                SectionHeader(title: String(localized: "Releases"))
                ForEach(detail.releases, id: \.releaseId) { release in
                    Button(action: {
                        openAlbum(release.albumId, release.releaseId)
                    }) {
                        HStack(spacing: 12) {
                            ImageView(imageRef: release.cover, pointSize: 42)
                                .frame(width: 42, height: 42)
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                            VStack(alignment: .leading, spacing: 3) {
                                Text(release.albumTitle)
                                    .font(.body)
                                    .lineLimit(1)
                                StableOptionalText(
                                    text: workReleaseMetadata(release),
                                    font: .caption,
                                    foreground: .secondary,
                                    lineHeight: 12,
                                    lineLimit: 1
                                )
                            }
                            Spacer()
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
            if !detail.tracks.isEmpty {
                SectionHeader(title: String(localized: "Recordings"))
                ForEach(detail.tracks, id: \.trackId) { track in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(track.trackTitle)
                            .font(.body)
                            .lineLimit(1)
                        Text(track.albumTitle)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
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

#if DEBUG
    extension LibraryView {
        /// A `Library` whose album/composer counts and pages are all empty,
        /// driving the real `loadInitial()` → `totalCount == 0` path so the
        /// previews below hit the actual empty-state branches in
        /// `albumContent`/`composerContent` rather than a hand-built stand-in.
        fileprivate static var emptyLibrary: Library {
            Library(
                getAlbumCount: { 0 },
                getAlbumPage: { _, _, _ in [] },
                getComposerCount: { 0 },
                getComposerPage: { _, _, _ in [] },
                getArtistCount: { 0 },
                getArtistPage: { _, _, _ in [] },
            )
        }
    }

    #Preview("Albums \u{2014} Empty") {
        LibraryView()
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(LibraryView.emptyLibrary)
            .environment(LibraryStore())
            .environment(UiStore())
            .environment(ProjectionRegistry())
            .frame(width: 1100, height: 700)
            .windowBackground()
    }

    #Preview("Composers \u{2014} Empty") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.composers)
        return LibraryView()
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(LibraryView.emptyLibrary)
            .environment(LibraryStore())
            .environment(uiStore)
            .environment(ProjectionRegistry())
            .frame(width: 1100, height: 700)
            .windowBackground()
    }

    #Preview("Artists \u{2014} Empty") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.artists)
        return LibraryView()
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(LibraryView.emptyLibrary)
            .environment(LibraryStore())
            .environment(uiStore)
            .environment(ProjectionRegistry())
            .frame(width: 1100, height: 700)
            .windowBackground()
    }
#endif
