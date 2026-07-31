import BaeKit
import Foundation

/// An album's release group with the pressings the search / auto-identify
/// surfaced for it, plus the display labels the group card renders. Mirrors
/// `BridgeReleaseGroup` — grouping and label formatting happen in bae-core;
/// the UI iterates the groups and their pressings and renders.
struct ReleaseGroup: Equatable, Identifiable {
    let id: String
    let title: String
    let artist: String?
    let coverArt: BridgeRemoteCover?
    /// Human-readable source name ("MusicBrainz" / "Discogs"), pre-built by core.
    let sourceLabel: String
    /// Editorial URL for the group on its source. `nil` for an ungrouped result.
    let groupUrl: URL?
    /// Pre-formatted year span + pressing count, e.g. "1992 – 2012 · 4 pressings".
    let metaLabel: String
    let pressings: [BridgeMetadataResult]

    var coverImageContent: ImageContent? {
        coverArt.map { ImageContent(bridge: $0.coverChoice.thumbnailSource) }
    }

    init(bridge: BridgeReleaseGroup) {
        id = bridge.id
        title = bridge.title
        artist = bridge.artist
        coverArt = bridge.coverArt
        sourceLabel = bridge.sourceLabel
        groupUrl = bridge.groupUrl.flatMap(URL.init(string:))
        metaLabel = bridge.metaLabel
        pressings = bridge.pressings
    }
}
