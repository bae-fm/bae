import Foundation

public struct SearchResults: Equatable, Sendable {
    public let query: String
    public let albums: [AlbumSearchResult]
    public let tracks: [TrackSearchResult]
    public let composers: [BridgeComposerSummary]
    public let works: [BridgeWorkSummary]

    public init(bridge: BridgeSearchResults, query: String) {
        self.query = query
        albums = bridge.albums.map(AlbumSearchResult.init(bridge:))
        tracks = bridge.tracks.map(TrackSearchResult.init(bridge:))
        composers = bridge.composers
        works = bridge.works
    }
}

public struct AlbumSearchResult: Equatable, Identifiable, Sendable {
    public let id: String
    public let title: String
    public let year: Int32?
    public let artistName: String
    public let cover: BridgeImageRef?

    public init(bridge: BridgeAlbumSearchResult) {
        id = bridge.id
        title = bridge.title
        year = bridge.year
        artistName = bridge.artistName
        cover = bridge.cover
    }
}

public struct TrackSearchResult: Equatable, Identifiable, Sendable {
    public let id: String
    public let title: String
    public let durationMs: Int64?
    public let albumId: String
    public let albumTitle: String
    public let artistName: String

    public var durationLabel: String { DurationClock.text(durationMs) }

    public init(bridge: BridgeTrackSearchResult) {
        id = bridge.id
        title = bridge.title
        durationMs = bridge.durationMs
        albumId = bridge.albumId
        albumTitle = bridge.albumTitle
        artistName = bridge.artistName
    }
}

extension BridgeWorkSummary: Identifiable {
    public var id: String { workId }
}
