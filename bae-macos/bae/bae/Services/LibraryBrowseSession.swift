import BaeKit
import Foundation
import Observation

/// The album grid's multi-selection read live through one query, opened on
/// the first selection: each selected album's summary is kept current in the
/// library store — what the bulk actions act on — and an album the read finds
/// gone leaves the selection. A new selection moves the same query.
@MainActor
private final class SelectedAlbums {
    private let library: Library
    private let libraryStore: LibraryStore
    private let uiStore: UiStore
    private weak var selection: AlbumGridSelection?
    private var query: AlbumSelectionQuery?
    private var deliveries: Task<Void, Never>?

    init(library: Library, libraryStore: LibraryStore, uiStore: UiStore) {
        self.library = library
        self.libraryStore = libraryStore
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
        for album in snapshot.albums {
            _ = libraryStore.internAlbumSummary(album)
        }
        let present = Set(snapshot.albums.map(\.id))
        let gone = snapshot.requested.filter { !present.contains($0) }
        guard !gone.isEmpty else { return }
        for albumId in gone {
            libraryStore.applyAlbumDetailSnapshot(albumId: albumId, bridge: nil)
        }
        selection?.remove(gone)
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
        libraryStore: LibraryStore,
        uiStore: UiStore
    ) {
        let selectedAlbums = SelectedAlbums(
            library: library,
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        self.albumSelection = selectedAlbums.makeSelection()
        albums = BrowseListSlot(
            defaultsKey: "librarySortCriteria",
            defaultCriteria: [
                BridgeSortCriterion(field: .dateAdded, direction: .descending)
            ],
            makePageSource: {
                LibraryAlbumPageSource(library: library, sort: $0)
            },
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row)
                }
            },
            onSnapshot: { _, total in
                libraryStore.setAlbumTotal(total)
            },
            onError: { uiStore.showError($0) }
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
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internComposerSummary(row)
                }
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
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internArtistSummary(row)
                }
            },
            onError: { uiStore.showError($0) }
        )
    }

    func start() {
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
