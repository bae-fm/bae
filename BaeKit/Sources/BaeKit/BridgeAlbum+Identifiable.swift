import Foundation

extension BridgeAlbum: Identifiable {}

extension BridgeAlbumAppearance: Identifiable {
    public var id: String { album.id }
}

/// The library grid's list, including the credits to show in each artist group.
public typealias AlbumList = PaginatedList<BridgeAlbumAppearance>
