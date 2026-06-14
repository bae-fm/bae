import Foundation

enum MetadataSource: Equatable, Hashable {
    case musicBrainz
    case discogs

    init(bridge: BridgeMetadataSource) {
        switch bridge {
        case .musicBrainz: self = .musicBrainz
        case .discogs: self = .discogs
        }
    }

    var bridge: BridgeMetadataSource {
        switch self {
        case .musicBrainz: .musicBrainz
        case .discogs: .discogs
        }
    }
}

struct MetadataResult: Equatable, Identifiable {
    let source: MetadataSource
    let releaseId: String
    let title: String
    let artist: String?
    let year: Int32?
    let format: String?
    let label: String?
    let catalogNumber: String?
    let country: String?
    let coverImageSource: ImageLoader.Source?
    let releaseUrl: String?

    var id: String {
        releaseId
    }

    init(bridge: BridgeMetadataResult) {
        source = MetadataSource(bridge: bridge.source)
        releaseId = bridge.releaseId
        title = bridge.title
        artist = bridge.artist
        year = bridge.year
        format = bridge.format
        label = bridge.label
        catalogNumber = bridge.catalogNumber
        country = bridge.country
        coverImageSource = bridge.coverUrl.map { .remote(url: $0) }
        releaseUrl = bridge.releaseUrl
    }
}

struct CoverArt: Equatable {
    let url: String
    /// Carried back to core when the user picks this cover (the commit path).
    let source: MetadataSource
    /// Pre-built source name from bae-core ("Cover Art Archive" / "Discogs"),
    /// rendered as-is — the UI never switches on `source` for display.
    let sourceLabel: String

    init(bridge: BridgeCoverArt) {
        url = bridge.url
        source = MetadataSource(bridge: bridge.source)
        sourceLabel = bridge.sourceLabel
    }
}
