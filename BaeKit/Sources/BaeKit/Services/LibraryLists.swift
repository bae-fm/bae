import Foundation
import Observation

// MARK: - Row identities

extension BridgeComposerSummary: Identifiable {
    public var id: String { artistId }
}

/// A composer page row's `id` is core's: the row's number on the page.
extension BridgeComposerDetailRow: Identifiable {}

extension BridgeArtistSummary: Identifiable {
    public var id: String { artistId }
}

extension BridgeStorageRow: Identifiable {
    public var id: String {
        release.id
    }
}

// MARK: - Lists

public typealias ComposerList = PaginatedList<BridgeComposerSummary>
/// A composer's page rows.
public typealias ComposerDetailRowList = PaginatedList<BridgeComposerDetailRow>
public typealias ArtistList = PaginatedList<BridgeArtistSummary>

/// The storage manager's paginated list; each row draws the
/// `BridgeStorageRow` (a release and its album) the manager's query returned
/// at its position.
public typealias StorageList = PaginatedList<BridgeStorageRow>

// MARK: - BridgeAlbum: Identifiable

/// `BridgeAlbum` already has an `id: String` — lift it into the
/// `Identifiable` protocol so `PaginatedList<BridgeAlbum>` compiles.
extension BridgeAlbum: Identifiable {}

/// The library grid's list; each card draws the `BridgeAlbum` the grid's
/// windows delivered at its position.
public typealias AlbumList = PaginatedList<BridgeAlbum>
