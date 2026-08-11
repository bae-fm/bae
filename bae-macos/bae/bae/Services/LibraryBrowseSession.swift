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

/// The library browser's session state: the album/composer/artist list slots,
/// each holding its live `PaginatedList`, invalidation registration, and sort
/// criteria (see `BrowseListSlot`), plus the current selections. Constructed
/// once at the app root, alongside `UiStore` — so unmounting `LibraryView` on
/// a tab switch loses none of it. The lists stay warm (no reload flash on
/// remount) and the selections persist; `LibraryView` reads this session and
/// calls its methods rather than owning the state itself.
///
/// Detail payloads derived from the selections (`composerPaneDetail`,
/// `artistDetail`) are deliberately NOT here — they stay `@State` on
/// `LibraryView` and cheaply re-fetch on remount. Losing a selection would be
/// a bug; a brief detail reload is not.
@MainActor
@Observable
final class LibraryBrowseSession {
    let albums: BrowseListSlot<BridgeAlbum, BridgeSortCriterion>
    let composers:
        BrowseListSlot<BridgeComposerSummary, BridgeComposerSortCriterion>
    let artists: BrowseListSlot<BridgeArtistSummary, BridgeArtistSortCriterion>

    /// Album-grid multi-selection, a browsing-session concern owned here (the
    /// Storage Manager precedent) and passed down to the grid.
    let albumSelection = AlbumGridSelection()
    /// The composers browser's detail-pane target. Views read it and call the
    /// `select…` methods below to change it — they never assign it directly.
    private(set) var detailSelection: ComposerPaneSelection = .none
    /// The selected artist in the artists browser, or `nil` before any
    /// selection. Views read it and call `selectArtist(_:)`.
    private(set) var selectedArtistId: String?

    init(
        library: Library,
        projectionRegistry: ProjectionRegistry,
        libraryStore: LibraryStore,
        uiStore: UiStore
    ) {
        albums = BrowseListSlot(
            defaultsKey: "librarySortCriteria",
            defaultCriteria: [
                BridgeSortCriterion(field: .dateAdded, direction: .descending)
            ],
            domain: .albumList,
            projectionRegistry: projectionRegistry,
            makePageSource: {
                LibraryAlbumPageSource(library: library, sort: $0)
            },
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row)
                }
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
            domain: .composerList,
            projectionRegistry: projectionRegistry,
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
            domain: .artistList,
            projectionRegistry: projectionRegistry,
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
