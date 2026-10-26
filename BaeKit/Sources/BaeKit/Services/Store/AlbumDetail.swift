import Foundation

/// One album as its detail read returned it: the album, and each of its
/// releases in album order, the primary release first.
public struct AlbumDetail: Sendable {
    public let album: BridgeAlbum
    public let releases: [ReleaseDetail]

    public init(bridge: BridgeAlbumDetail) {
        album = bridge.album
        releases = bridge.releases.map(ReleaseDetail.init(bridge:))
    }

    /// The release `id` of this album, or nil when the album holds no such
    /// release.
    public func release(_ id: String) -> ReleaseDetail? {
        releases.first { $0.id == id }
    }
}
