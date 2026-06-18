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
    let coverArt: [BridgeRemoteCover]

    init(bridge: BridgeReleaseDetail) {
        releaseId = bridge.releaseId
        trackCount = bridge.trackCount
        trackCountMismatch = bridge.trackCountMismatch
        coverArt = bridge.coverArt
    }
}
