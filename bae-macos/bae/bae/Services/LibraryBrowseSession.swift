import BaeKit
import Foundation
import Observation

/// The album grid's multi-selection read live through one query, opened on
/// the first selection: an album the read finds gone leaves the selection,
/// wherever in the grid it sits, loaded or not. A new selection moves the
/// same query.
@MainActor
private final class SelectedAlbums {
    private let library: Library
    private let uiStore: UiStore
    private weak var selection: AlbumGridSelection?
    private var query: AlbumSelectionQuery?
    private var deliveries: Task<Void, Never>?

    init(library: Library, uiStore: UiStore) {
        self.library = library
        self.uiStore = uiStore
    }

    func makeSelection() -> AlbumGridSelection {
        precondition(self.selection == nil)
        let selection = AlbumGridSelection { [self] selectedIds in
            selectionChanged(selectedIds)
        }
        self.selection = selection
        return selection
    }

    func selectionChanged(_ selectedIds: Set<String>) {
        if selectedIds.isEmpty && query == nil { return }
        do {
            try (query ?? open()).setAlbums(selectedIds.sorted())
        }
        catch {
            uiStore.showError(error)
        }
    }

    private func open() -> AlbumSelectionQuery {
        let query = library.albumSelection()
        self.query = query
        deliveries = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let snapshot = try await query.next()
                    guard let self else { return }
                    self.deliver(snapshot)
                }
                catch BridgeError.Cancelled {
                    return
                }
                catch {
                    if !Task.isCancelled { self?.uiStore.showError(error) }
                    return
                }
            }
        }
        return query
    }

    private func deliver(_ snapshot: BridgeAlbumSelectionSnapshot) {
        guard !snapshot.gone.isEmpty else { return }
        selection?.remove(snapshot.gone)
    }

    deinit {
        deliveries?.cancel()
        if let query {
            Task { await query.cancel() }
        }
    }
}

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

/// The library browser's session state: the album/composer/artist list slots,
/// each holding its live `PaginatedList`, subscription, and sort
/// criteria (see `BrowseListSlot`), plus the current selections. Constructed
/// once at the app root, alongside `UiStore` — so unmounting `LibraryView` on
/// a tab switch loses none of it. The lists stay warm (no reload flash on
/// remount) and the selections persist; `LibraryView` reads this session and
/// calls its methods rather than owning the state itself.
///
/// Detail payload subscriptions derived from these selections live in the
/// app-owned `LibraryProjectionStore`; the view reports selection changes and
/// renders its delivered values.
@MainActor
@Observable
final class LibraryBrowseSession {
    let albumGrouping: AlbumGrouping
    let albums: BrowseListSlot<BridgeAlbum, BridgeSortCriterion>
    let composers:
        BrowseListSlot<BridgeComposerSummary, BridgeComposerSortCriterion>
    let artists: BrowseListSlot<BridgeArtistSummary, BridgeArtistSortCriterion>

    /// Album-grid multi-selection, a browsing-session concern owned here (the
    /// Storage Manager precedent) and passed down to the grid.
    let albumSelection: AlbumGridSelection
    /// The composers browser's detail-pane target. Views read it and call the
    /// `select…` methods below to change it — they never assign it directly.
    private(set) var detailSelection: ComposerPaneSelection = .none
    /// The selected artist in the artists browser, or `nil` before any
    /// selection. Views read it and call `selectArtist(_:)`.
    private(set) var selectedArtistId: String?

    init(
        library: Library,
        uiStore: UiStore
    ) {
        let selectedAlbums = SelectedAlbums(
            library: library,
            uiStore: uiStore
        )
        self.albumSelection = selectedAlbums.makeSelection()
        let albumGrouping = AlbumGrouping()
        self.albumGrouping = albumGrouping
        albums = Self.makeAlbumList(
            library: library,
            uiStore: uiStore,
            grouping: albumGrouping
        )
        composers = BrowseListSlot(
            defaultsKey: "libraryComposerSortCriteria",
            defaultCriteria: [
                BridgeComposerSortCriterion(
                    field: .name,
                    direction: .ascending
                )
            ],
            makePageSource: {
                LibraryComposerPageSource(library: library, sort: $0)
            },
            onError: { uiStore.showError($0) }
        )
        artists = BrowseListSlot(
            defaultsKey: "libraryArtistSortCriteria",
            defaultCriteria: [
                BridgeArtistSortCriterion(field: .name, direction: .ascending)
            ],
            makePageSource: {
                LibraryArtistPageSource(library: library, sort: $0)
            },
            onError: { uiStore.showError($0) }
        )
    }

    private static func makeAlbumList(
        library: Library,
        uiStore: UiStore,
        grouping: AlbumGrouping
    ) -> BrowseListSlot<BridgeAlbum, BridgeSortCriterion> {
        BrowseListSlot(
            defaultsKey: "librarySortCriteria",
            defaultCriteria: [
                BridgeSortCriterion(field: .dateAdded, direction: .descending)
            ],
            makePageSource: {
                LibraryAlbumPageSource(
                    library: library,
                    sort: $0,
                    groupByArtist: grouping.byArtist
                )
            },
            onError: { uiStore.showError($0) }
        )
    }

    /// How many albums the open library holds, as the album list's query
    /// counted them, or `nil` before it has. Read by chrome that has no list
    /// of its own (the Playback menu's Shuffle Library item disables at zero,
    /// and an empty composer or artist list says why). Albums are the proxy
    /// the UI owns for "anything to play": core's shuffle no-ops on zero
    /// *tracks*, but the UI never loads a track count.
    var albumTotal: Int? {
        albums.list?.itemCount
    }

    func start() {
        albums.startLoad()
    }

    func setGroupAlbumsByArtist(_ enabled: Bool) {
        albumGrouping.setByArtist(enabled)
        albums.startLoad()
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
}
