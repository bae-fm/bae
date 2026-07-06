import Foundation

struct Track: Identifiable {
    let id: String
    var title: String
    var durationMs: Int64?
    var artistNames: String
    var positionText: String

    var durationLabel: String { DurationClock.text(durationMs) }

    init(from bridge: BridgeTrack) {
        id = bridge.id
        title = bridge.title
        durationMs = bridge.durationMs
        artistNames = bridge.artistNames
        positionText = bridge.positionText
    }
}
