import Foundation
import Observation

private final class TaskPageSubscription: PageSubscription,
    @unchecked Sendable
{
    private let task: Task<Void, Never>

    init(_ task: Task<Void, Never>) {
        self.task = task
    }

    func cancel() {
        task.cancel()
    }
}

// MARK: - Album page sources

/// The full library grid's pages under one sort, served by one browse
/// query over the windows the visible pages ask for.
public typealias LibraryAlbumPageSource = LibraryBrowsePageSource<BridgeAlbum>

extension LibraryBrowsePageSource where Row == BridgeAlbum {
    public convenience init(
        library: Library,
        sort: [BridgeSortCriterion],
        groupByArtist: Bool = false
    ) {
        self.init(query: library.albumBrowse(sort, groupByArtist))
    }
}

/// In-memory page source for album previews and tests. Expects rows to
/// arrive pre-sorted; serves contiguous slices.
public struct AlbumPreviewPageSource: PageSource {
    public let albums: [BridgeAlbum]

    public init(albums: [BridgeAlbum]) {
        self.albums = albums
    }

    public func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([BridgeAlbum], Int) -> Void,
        onError _: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let albums = albums
        return TaskPageSubscription(
            Task { @MainActor in
                let start = min(offset, albums.count)
                let end = min(start + limit, albums.count)
                onValue(Array(albums[start..<end]), albums.count)
            }
        )
    }
}

public typealias LibraryComposerPageSource = LibraryBrowsePageSource<
    BridgeComposerSummary
>

extension LibraryBrowsePageSource where Row == BridgeComposerSummary {
    public convenience init(
        library: Library,
        sort: [BridgeComposerSortCriterion]
    ) {
        self.init(query: library.composerBrowse(sort))
    }
}

extension BridgeComposerSummary: Identifiable {
    public var id: String { artistId }
}

public typealias LibraryArtistPageSource = LibraryBrowsePageSource<
    BridgeArtistSummary
>

extension LibraryBrowsePageSource where Row == BridgeArtistSummary {
    public convenience init(
        library: Library,
        sort: [BridgeArtistSortCriterion]
    ) {
        self.init(query: library.artistBrowse(sort))
    }
}

extension BridgeArtistSummary: Identifiable {
    public var id: String { artistId }
}

extension BridgeStorageRow: Identifiable {
    public var id: String {
        release.id
    }
}

// MARK: - Lists

/// The library grid's paginated list; each card draws the `BridgeAlbum` the
/// grid's query returned at its position.
public typealias AlbumList = PaginatedList<BridgeAlbum>
public typealias ComposerList = PaginatedList<BridgeComposerSummary>
public typealias ArtistList = PaginatedList<BridgeArtistSummary>

/// The storage manager's paginated list; each row draws the
/// `BridgeStorageRow` (a release and its album) the manager's query returned
/// at its position.
public typealias StorageList = PaginatedList<BridgeStorageRow>

extension AlbumList {
    /// A pre-populated list for SwiftUI previews and tests, seeded with
    /// `albums` so the preview renders without waiting for an async
    /// round-trip.
    ///
    /// Rows render in the order given. The live grid is ordered by the database,
    /// through the `ORDER BY` its page query builds; a preview holds a hand-written
    /// list with no database, so it presents the order its author wrote.
    public static func preview(albums: [BridgeAlbum]) -> AlbumList {
        let list = AlbumList(
            pageSource: AlbumPreviewPageSource(albums: albums),
            onError: { _ in },
        )
        list.preloadForPreview(rows: albums)
        return list
    }
}

// MARK: - BridgeAlbum: Identifiable

/// `BridgeAlbum` already has an `id: String` — lift it into the
/// `Identifiable` protocol so `PaginatedList<BridgeAlbum>` compiles.
extension BridgeAlbum: Identifiable {}
