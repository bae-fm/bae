import Foundation

struct QueueItem: Identifiable, Equatable {
    let trackId: String
    let title: String
    let durationMs: Int64?
    let albumTitle: String
    let coverImageId: String?

    var id: String {
        trackId
    }

    var durationLabel: String { DurationClock.text(durationMs) }

    init(bridge: BridgeQueueItem) {
        trackId = bridge.trackId
        title = bridge.title
        durationMs = bridge.durationMs
        albumTitle = bridge.albumTitle
        coverImageId = bridge.coverImageId
    }
}
