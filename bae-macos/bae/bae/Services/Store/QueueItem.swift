import Foundation

struct QueueItem: Identifiable, Equatable {
    let trackId: String
    let title: String
    let durationLabel: String
    let albumTitle: String
    let coverImageId: String?

    var id: String {
        trackId
    }

    init(bridge: BridgeQueueItem) {
        trackId = bridge.trackId
        title = bridge.title
        durationLabel = bridge.durationLabel
        albumTitle = bridge.albumTitle
        coverImageId = bridge.coverImageId
    }
}
