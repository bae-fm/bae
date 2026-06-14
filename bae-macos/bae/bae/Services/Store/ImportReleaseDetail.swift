import Foundation

/// Release detail as returned by the auto-identify / search pipeline
/// (MusicBrainz, Discogs). Distinct from the library-side release
/// detail (`ReleaseDetail` in the `releases` slice): this one carries
/// external metadata about a candidate, not a release stored in the
/// library.
struct ImportReleaseDetail: Equatable {
    let releaseId: String
    let trackCount: UInt32
    let trackCountMismatch: Bool
    let coverArt: [CoverArt]
    /// The source's primary cover URL, pre-computed by core: what the
    /// picker shows selected when the confirm pane mounts without a
    /// manual pick.
    let defaultCoverUrl: String?

    init(bridge: BridgeReleaseDetail) {
        releaseId = bridge.releaseId
        trackCount = bridge.trackCount
        trackCountMismatch = bridge.trackCountMismatch
        coverArt = bridge.coverArt.map(CoverArt.init(bridge:))
        defaultCoverUrl = bridge.defaultCoverUrl
    }
}
