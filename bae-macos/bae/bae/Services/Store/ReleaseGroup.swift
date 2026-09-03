import BaeKit
import Foundation

/// An album, as one or both sources describe it, with the pressings they
/// surfaced for it. Mirrors `BridgeReleaseGroup` — the grouping, the
/// cross-source merge and the pressing pairing all happen in bae-core; the UI
/// iterates the groups and their pressings and renders.
struct ReleaseGroup: Equatable, Identifiable {
    let id: String
    let title: String
    let artist: String?
    /// The label core names for the album, where its pressings state one.
    let label: String?
    let coverArt: BridgeRemoteCover?
    /// Every source carrying this group, MusicBrainz first.
    let sources: [BridgeReleaseGroupSource]
    /// One row per physical pressing, each carrying every source that lists it.
    let pressings: [Pressing]

    var coverImageContent: ImageContent? {
        coverArt?.coverChoice.thumbnailContent
    }

    init(bridge: BridgeReleaseGroup) {
        id = bridge.id
        title = bridge.title
        artist = bridge.artist
        label = bridge.label
        coverArt = bridge.coverArt
        sources = bridge.sources
        pressings = bridge.pressings.compactMap(Pressing.init(bridge:))
    }
}
