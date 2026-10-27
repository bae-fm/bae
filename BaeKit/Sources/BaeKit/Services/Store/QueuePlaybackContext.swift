import Foundation

/// A queue row. `entryId` is per-instance: the same track queued twice yields
/// two entries with two ids, so the row identity is stable and unique even for
/// duplicates. `trackId` is what a cross-lane drag enqueues (a context row
/// dropped into "Up Next" inserts the track; entry ids only ever address their
/// own lane's instance).
extension BridgeQueueEntry: Identifiable {
    public var id: String {
        entryId
    }

    public var durationLabel: String { DurationClock.label(durationClock) }
}

/// The context lane (what the queue is playing from): its kind (a release vs
/// the whole library, which the section header labels) and whether it was
/// ordered by shuffle (rendered as a shuffle indicator). Shown as its own
/// section, distinct from the manual "Up Next" lane. Its tracks are read
/// through `PlaybackStore`'s upcoming read, not carried here.
public struct QueuePlaybackContext: Equatable, Sendable {
    public let kind: BridgePlaybackSourceKind
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `nil` for a multi-release source or the
    /// whole library. The UI appends it to the localized section label.
    public let sourceTitle: String?
    public let shuffled: Bool

    public init(bridge: BridgePlaybackContext) {
        kind = bridge.kind
        sourceTitle = bridge.sourceTitle
        shuffled = bridge.shuffled
    }
}
