import Foundation

struct QueueItem: Identifiable, Equatable {
    /// Per-instance id: the same track queued twice yields two items with two
    /// ids, so the row identity is stable and unique even for duplicates.
    let entryId: String
    let title: String
    let durationMs: Int64?
    let albumTitle: String
    let coverImageId: String?

    var id: String {
        entryId
    }

    var durationLabel: String { DurationClock.text(durationMs) }

    init(bridge: BridgeQueueEntry) {
        entryId = bridge.entryId
        title = bridge.title
        durationMs = bridge.durationMs
        albumTitle = bridge.albumTitle
        coverImageId = bridge.coverImageId
    }
}

/// The context lane (the release being played from): its not-yet-played tail,
/// plus whether it was ordered by shuffle (rendered as a shuffle indicator).
/// Shown as its own section, distinct from the manual "Up Next" lane.
struct QueuePlaybackContext: Equatable {
    let shuffled: Bool
    let upcoming: [QueueItem]

    init(bridge: BridgePlaybackContext) {
        shuffled = bridge.shuffled
        upcoming = bridge.upcoming.map(QueueItem.init(bridge:))
    }

    init(shuffled: Bool, upcoming: [QueueItem]) {
        self.shuffled = shuffled
        self.upcoming = upcoming
    }
}
