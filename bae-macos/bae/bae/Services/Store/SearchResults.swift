import Foundation

struct SearchResults: Equatable {
    let query: String
    let albums: [AlbumSearchResult]
    let tracks: [TrackSearchResult]
    let composers: [BridgeComposerSummary]
    let works: [BridgeWorkSummary]

    init(bridge: BridgeSearchResults, query: String) {
        self.query = query
        albums = bridge.albums.map(AlbumSearchResult.init(bridge:))
        tracks = bridge.tracks.map(TrackSearchResult.init(bridge:))
        composers = bridge.composers
        works = bridge.works
    }
}

struct AlbumSearchResult: Equatable, Identifiable {
    let id: String
    let title: String
    let year: Int32?
    let artistName: String
    let cover: BridgeImageRef?

    init(bridge: BridgeAlbumSearchResult) {
        id = bridge.id
        title = bridge.title
        year = bridge.year
        artistName = bridge.artistName
        cover = bridge.cover
    }
}

struct TrackSearchResult: Equatable, Identifiable {
    let id: String
    let title: String
    let durationMs: Int64?
    let albumId: String
    let albumTitle: String
    let artistName: String

    var durationLabel: String { DurationClock.text(durationMs) }

    init(bridge: BridgeTrackSearchResult) {
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
