import Foundation

public struct QueueItem: Identifiable, Equatable, Sendable {
    /// Per-instance id: the same track queued twice yields two items with two
    /// ids, so the row identity is stable and unique even for duplicates.
    public let entryId: String
    public let title: String
    public let durationMs: Int64?
    public let albumTitle: String
    public let coverImageId: String?

    public var id: String {
        entryId
    }

    public var durationLabel: String { DurationClock.text(durationMs) }

    public init(bridge: BridgeQueueEntry) {
        entryId = bridge.entryId
        title = bridge.title
        durationMs = bridge.durationMs
        albumTitle = bridge.albumTitle
        coverImageId = bridge.coverImageId
    }
}

/// The context lane (what the queue is playing from): its kind (a release vs the
/// whole library, which the section header labels), the first page of its
/// not-yet-played tail, the tail's full length, plus whether it was ordered by
/// shuffle (rendered as a shuffle indicator). Shown as its own section, distinct
/// from the manual "Up Next" lane.
///
/// `upcoming` is only the initial window core resolved eagerly — not the whole
/// tail, which is library-scaled. Indices at or past `upcoming.count` (and below
/// `upcomingTotal`) are unloaded until `PlaybackStore.loadUpcomingRange` fetches
/// them; read them via `PlaybackStore.upcomingItem(at:)`, not this array
/// directly.
public struct QueuePlaybackContext: Equatable, Sendable {
    public let kind: BridgePlaybackSourceKind
    public let shuffled: Bool
    public let upcoming: [QueueItem]
    public let upcomingTotal: Int

    public init(bridge: BridgePlaybackContext) {
        kind = bridge.kind
        shuffled = bridge.shuffled
        upcoming = bridge.upcoming.map(QueueItem.init(bridge:))
        upcomingTotal = Int(bridge.upcomingTotal)
    }

    public init(
        kind: BridgePlaybackSourceKind,
        shuffled: Bool,
        upcoming: [QueueItem],
        upcomingTotal: Int
    ) {
        self.kind = kind
        self.shuffled = shuffled
        self.upcoming = upcoming
        self.upcomingTotal = upcomingTotal
    }
}
