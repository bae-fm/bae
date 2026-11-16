import Foundation

/// `BridgeAlbum` already has an `id: String`, so it conforms to
/// `Identifiable` with no new code, which `PaginatedList<BridgeAlbum>` needs.
extension BridgeAlbum: Identifiable {}

/// The library grid's list; each card draws the `BridgeAlbum` the grid's
/// windows delivered at its position.
public typealias AlbumList = PaginatedList<BridgeAlbum>
