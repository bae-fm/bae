import Foundation

/// One album as its detail read returned it: the album, and each of its
/// releases in album order, the primary release first. Core reads no album
/// detail without a release, and names the album's release ids from these
/// same releases.
public struct AlbumDetail: Sendable {
    public let album: BridgeAlbum
    public let releases: [ReleaseDetail]
    /// The release the album plays by default and shows the cover of.
    public let primaryRelease: ReleaseDetail

    public init(bridge: BridgeAlbumDetail) {
        album = bridge.album
        releases = bridge.releases.map(ReleaseDetail.init(bridge:))
        guard
            let primary = releases.first(where: {
                $0.id == bridge.album.primaryReleaseId
            })
        else {
            preconditionFailure(
                "Album \(bridge.album.id)'s primary release \(bridge.album.primaryReleaseId) is not one of its releases"
            )
        }
        primaryRelease = primary
    }

    /// The release `id` of this album, or nil when the album holds no such
    /// release.
    public func release(_ id: String) -> ReleaseDetail? {
        releases.first { $0.id == id }
    }
}
