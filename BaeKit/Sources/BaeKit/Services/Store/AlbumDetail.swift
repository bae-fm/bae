import Foundation

/// One album as its detail read returned it: the album, each of its releases
/// in album order with the primary release first, and the release the page
/// shows. Core reads no album detail without a release.
public struct AlbumDetail: Sendable {
    public let album: BridgeAlbum
    public let releases: [ReleaseDetail]
    /// Who rated the album, and whether this person can.
    public let rating: BridgeAlbumRating
    /// The release the page shows: the one its request chose while the album
    /// holds it, else the primary release.
    public let shownRelease: ReleaseDetail
    /// The shown release's place in `releases`, which the release picker
    /// marks.
    public let shownReleaseIndex: Int
    /// Whether the shown release is not the primary release, so the page
    /// offers Set as primary release.
    public let shownReleaseCanBecomePrimary: Bool
    /// Whether the album has more than one release, so its page offers a
    /// choice between them.
    public let showsReleaseChoices: Bool

    public init(bridge: BridgeAlbumDetail) {
        album = bridge.album
        releases = bridge.releases.map(ReleaseDetail.init(bridge:))
        rating = bridge.rating
        shownRelease = ReleaseDetail(bridge: bridge.shownRelease)
        shownReleaseIndex = Int(bridge.shownReleaseIndex)
        shownReleaseCanBecomePrimary = bridge.shownReleaseCanBecomePrimary
        showsReleaseChoices = bridge.showsReleaseChoices
    }
}
