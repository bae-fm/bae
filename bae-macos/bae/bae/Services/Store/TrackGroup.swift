import Foundation

struct TrackGroup {
    var sideLabel: String
    var tracks: [Track]

    init(from bridge: BridgeTrackGroup) {
        sideLabel = bridge.sideLabel
        tracks = bridge.tracks.map(Track.init(from:))
    }
}
