import Foundation

public struct QueueItem: Identifiable, Equatable, Sendable {
    /// Per-instance id: the same track queued twice yields two items with two
    /// ids, so the row identity is stable and unique even for duplicates.
    public let entryId: String
    /// The underlying track — what a cross-lane drag enqueues (a context row
    /// dropped into "Up Next" inserts the track; entry ids only ever address
    /// their own lane's instance).
    public let trackId: String
    public let title: String
    public let durationClock: BridgeDurationClock?
    public let albumTitle: String
    public let coverImage: BridgeImageRef?

    public var id: String {
        entryId
    }

    public var durationLabel: String { DurationClock.label(durationClock) }

    public init(bridge: BridgeQueueEntry) {
        entryId = bridge.entryId
        trackId = bridge.trackId
        title = bridge.title
        durationClock = bridge.durationClock
        albumTitle = bridge.albumTitle
        coverImage = bridge.coverImage
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
/// `upcomingTotal`) are unloaded until `PlaybackStore.loadUpcomingRange`
/// subscribes to them; read them via `PlaybackStore.upcomingItem(at:)`, not this
/// array directly.
public struct QueuePlaybackContext: Equatable, Sendable {
    public let kind: BridgePlaybackSourceKind
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `nil` for a multi-release source or the
    /// whole library. The UI appends it to the localized section label.
    public let sourceTitle: String?
    public let shuffled: Bool
    public let upcoming: [QueueItem]
    public let upcomingTotal: Int

    public init(bridge: BridgePlaybackContext) {
        kind = bridge.kind
        sourceTitle = bridge.sourceTitle
        shuffled = bridge.shuffled
        upcoming = bridge.upcoming.map(QueueItem.init(bridge:))
        upcomingTotal = Int(bridge.upcomingTotal)
    }

    public init(
        kind: BridgePlaybackSourceKind,
        sourceTitle: String?,
        shuffled: Bool,
        upcoming: [QueueItem],
        upcomingTotal: Int
    ) {
        self.kind = kind
        self.sourceTitle = sourceTitle
        self.shuffled = shuffled
        self.upcoming = upcoming
        self.upcomingTotal = upcomingTotal
    }
}
