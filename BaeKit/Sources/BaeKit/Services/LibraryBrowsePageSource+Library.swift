import Foundation
import Observation

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

// MARK: - BridgeAlbum: Identifiable

/// `BridgeAlbum` already has an `id: String` — lift it into the
/// `Identifiable` protocol so `PaginatedList<BridgeAlbum>` compiles.
extension BridgeAlbum: Identifiable {}
