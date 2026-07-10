import BaeKit
import Observation

// MARK: - ComposerPaneSelection

/// The composers browser's detail-pane target: nothing selected, a composer
/// (optionally drilled into one of its works), or a work reached directly (a
/// search result, a release's composer credit). Promoted out of `LibraryView`
/// alongside `LibraryBrowseSession` — navigation into this pane can be issued
/// while the library section is unmounted, so the selection must survive a
/// remount.
enum ComposerPaneSelection: Equatable {
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

// MARK: - LibraryBrowseSession

/// The library browser's session state: the album/composer/artist lists (and
/// their live invalidation registrations), the current selections, and the
/// sort criteria for each mode. Constructed once at the app root, alongside
/// `UiStore` — so unmounting `LibraryView` on a tab switch loses none of it.
/// The lists stay warm (no reload flash on remount) and the selections
/// persist; `LibraryView` reads this session and calls its methods rather
/// than owning the state itself.
///
/// Detail payloads derived from the selections (`composerPaneDetail`,
/// `artistDetail`) are deliberately NOT here — they stay `@State` on
/// `LibraryView` and cheaply re-fetch on remount. Losing a selection would be
/// a bug; a brief detail reload is not.
@MainActor
@Observable
final class LibraryBrowseSession {
    private let library: Library
    private let projectionRegistry: ProjectionRegistry
    private let libraryStore: LibraryStore
    private let uiStore: UiStore

    var albumList: AlbumList?
    private var albumListRegistration: ProjectionRegistration?
    private var albumReloadTask: Task<Void, Never>?

    var composerList: ComposerList?
    private var composerListRegistration: ProjectionRegistration?
    private var composerReloadTask: Task<Void, Never>?

    var artistList: ArtistList?
    private var artistListRegistration: ProjectionRegistration?
    private var artistReloadTask: Task<Void, Never>?

    /// Album-grid multi-selection, a browsing-session concern owned here (the
    /// Storage Manager precedent) and passed down to the grid.
    let albumSelection = AlbumGridSelection()
    /// The composers browser's detail-pane target. Views read it and call the
    /// `select…` methods below to change it — they never assign it directly.
    private(set) var detailSelection: ComposerPaneSelection = .none
    /// The selected artist in the artists browser, or `nil` before any
    /// selection. Views read it and call `selectArtist(_:)`.
    private(set) var selectedArtistId: String?

    private(set) var sortCriteria: [BridgeSortCriterion]
    private(set) var composerSortCriteria: [BridgeComposerSortCriterion]
    private(set) var artistSortCriteria: [BridgeArtistSortCriterion]

    init(
        library: Library,
        projectionRegistry: ProjectionRegistry,
        libraryStore: LibraryStore,
        uiStore: UiStore
    ) {
        self.library = library
        self.projectionRegistry = projectionRegistry
        self.libraryStore = libraryStore
        self.uiStore = uiStore
        sortCriteria = Self.loadSortCriteria()
        composerSortCriteria = Self.loadComposerSortCriteria()
        artistSortCriteria = Self.loadArtistSortCriteria()

        albumReloadTask = Task { [weak self] in
            guard let self else { return }
            await reloadAlbumList(sort: sortCriteria)
        }
    }

    // MARK: - Sort criteria

    func setSortCriteria(_ criteria: [BridgeSortCriterion]) {
        sortCriteria = criteria
        Self.saveSortCriteria(criteria)
        albumReloadTask?.cancel()
        albumReloadTask = Task { [weak self] in
            guard let self else { return }
            await reloadAlbumList(sort: criteria)
        }
    }

    func setComposerSortCriteria(_ criteria: [BridgeComposerSortCriterion]) {
        composerSortCriteria = criteria
        Self.saveComposerSortCriteria(criteria)
        composerReloadTask?.cancel()
        composerReloadTask = Task { [weak self] in
            guard let self else { return }
            await reloadComposerList(sort: criteria)
        }
    }

    func setArtistSortCriteria(_ criteria: [BridgeArtistSortCriterion]) {
        artistSortCriteria = criteria
        Self.saveArtistSortCriteria(criteria)
        artistReloadTask?.cancel()
        artistReloadTask = Task { [weak self] in
            guard let self else { return }
            await reloadArtistList(sort: criteria)
        }
    }

    // MARK: - Detail-pane selection

    /// Select a composer with no work drilled into yet (the composer's
    /// overview). Used by a composer-list click and by cross-section
    /// navigation into a composer.
    func selectComposer(_ artistId: String) {
        detailSelection = .composer(artistId: artistId, workId: nil)
    }

    /// Drill into a specific work within a composer, keeping the composer as
    /// the pane's context.
    func selectComposerWork(artistId: String, workId: String) {
        detailSelection = .composer(artistId: artistId, workId: workId)
    }

    /// Open a work directly, with no composer context (a work reached from a
    /// release credit or from another work's detail).
    func selectWork(_ workId: String) {
        detailSelection = .work(workId: workId)
    }

    /// Select an artist in the artists browser.
    func selectArtist(_ artistId: String) {
        selectedArtistId = artistId
    }

    // MARK: - Mode-switch loading

    /// Load the composer list the first time the user switches into composers
    /// mode; a no-op afterward, so a remount that re-fires this finds a warm
    /// list and does nothing.
    func ensureComposerListLoaded() async {
        guard composerList == nil else {
            return
        }
        await reloadComposerList(sort: composerSortCriteria)
    }

    /// Load the artist list the first time the user switches into artists
    /// mode; a no-op afterward, so a remount that re-fires this finds a warm
    /// list and does nothing.
    func ensureArtistListLoaded() async {
        guard artistList == nil else {
            return
        }
        await reloadArtistList(sort: artistSortCriteria)
    }

    // MARK: - List construction

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

    private func reloadComposerList(
        sort: [BridgeComposerSortCriterion]
    ) async {
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

    // MARK: - Sort criteria persistence

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
