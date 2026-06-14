import Foundation

struct Track: Identifiable {
    let id: String
    var title: String
    var durationLabel: String
    var artistNames: String
    var positionLabel: String

    init(from bridge: BridgeTrack) {
        id = bridge.id
        title = bridge.title
        durationLabel = bridge.durationLabel
        artistNames = bridge.artistNames
        positionLabel = bridge.positionLabel
    }
}
