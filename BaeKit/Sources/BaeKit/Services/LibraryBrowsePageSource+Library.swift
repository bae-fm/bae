import Foundation
import Observation

// MARK: - Page sources

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

/// One composer's page rows, served by one query over the windows the
/// visible pages ask for.
public typealias LibraryComposerDetailPageSource = LibraryBrowsePageSource<
    BridgeComposerDetailRow
>

extension LibraryBrowsePageSource where Row == BridgeComposerDetailRow {
    public convenience init(library: Library, artistId: String) {
        self.init(query: library.composerDetailRows(artistId))
    }
}

/// A composer page row's `id` is core's: the row's number on the page.
extension BridgeComposerDetailRow: Identifiable {}

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

public typealias ComposerList = OffsetPaginatedList<BridgeComposerSummary>
/// A composer's page rows, a page at a time.
public typealias ComposerDetailRowList = OffsetPaginatedList<
    BridgeComposerDetailRow
>
public typealias ArtistList = OffsetPaginatedList<BridgeArtistSummary>

/// The storage manager's paginated list; each row draws the
/// `BridgeStorageRow` (a release and its album) the manager's query returned
/// at its position.
public typealias StorageList = OffsetPaginatedList<BridgeStorageRow>

// MARK: - BridgeAlbum: Identifiable

/// `BridgeAlbum` already has an `id: String` — lift it into the
/// `Identifiable` protocol so `PaginatedList<BridgeAlbum>` compiles.
extension BridgeAlbum: Identifiable {}

/// The library grid's list; each card draws the `BridgeAlbum` the grid's
/// windows delivered at its position.
public typealias AlbumList = PaginatedList<BridgeAlbum>
