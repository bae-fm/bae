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
    let coverArt: BridgeRemoteCover?
    /// Every source carrying this group, MusicBrainz first.
    let sources: [BridgeReleaseGroupSource]
    /// Pre-formatted year span + pressing count, e.g. "1992 – 2012 · 4 pressings".
    let metaLabel: String
    /// One entry per pressing row, each already the release that picking the
    /// row commits: core orders a pressing's sources, MusicBrainz first.
    let pressings: [BridgeMetadataResult]

    /// The sources' names for the card's meta line ("MusicBrainz · Discogs").
    var sourceLabel: String {
        sources
            .map { bridgeMetadataSourceName(source: $0.source) }
            .joined(separator: " \u{00b7} ")
    }

    /// The editorial page to open from the card. The first source's, which is
    /// MusicBrainz's whenever it carries this group.
    var groupUrl: URL? {
        sources.lazy.compactMap(\.groupUrl).first.flatMap(URL.init(string:))
    }

    var coverImageContent: ImageContent? {
        coverArt?.coverChoice.thumbnailContent
    }

    init(bridge: BridgeReleaseGroup) {
        id = bridge.id
        title = bridge.title
        artist = bridge.artist
        coverArt = bridge.coverArt
        sources = bridge.sources
        metaLabel = bridge.metaLabel
        pressings = bridge.pressings.compactMap(\.releases.first)
    }
}
