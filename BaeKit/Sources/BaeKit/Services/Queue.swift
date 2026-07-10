import Foundation

/// Queue mutations — appending, inserting, reordering, removing,
/// jumping to a queue entry. Reorder/remove/skip target a per-instance
/// `entryId`. Narrow subset of `AppHandle`.
public final class Queue: Sendable, Observable {
    public let addToQueue: @Sendable (_ trackIds: [String]) -> Void
    public let addNext: @Sendable (_ trackIds: [String]) -> Void
    public let addReleaseToQueue: @Sendable (_ releaseId: String) -> Void
    public let addReleaseNext: @Sendable (_ releaseId: String) -> Void
    public let insertInQueue:
        @Sendable (_ trackIds: [String], _ index: UInt32) -> Void
    public let removeEntry: @Sendable (_ entryId: String) -> Void
    public let clearQueue: @Sendable () -> Void
    /// Move `entryId` to sit before `beforeEntryId`; `nil` moves it to the end.
    public let reorderEntry:
        @Sendable (_ entryId: String, _ beforeEntryId: String?) -> Void
    public let skipToEntry: @Sendable (_ entryId: String) -> Void
    /// Flip the playing context between sequential and shuffled order.
    public let setShuffle: @Sendable (_ on: Bool) -> Void
    /// Fetch one page of the context's upcoming tail past the initial window
    /// `get_queue_snapshot`/`QueueUpdated` already carry. `offset` 0 is the
    /// first not-yet-played entry after the current track. Called by
    /// `PlaybackStore.loadUpcomingRange` as the queue view scrolls; not a
    /// mutation, so it has no corresponding `QueueUpdated` event.
    public let getUpcomingPage:
        @Sendable (_ offset: UInt32, _ limit: UInt32) async throws ->
            BridgeQueueUpcomingPage

    public init(
        addToQueue: @escaping @Sendable ([String]) -> Void = { _ in },
        addNext: @escaping @Sendable ([String]) -> Void = { _ in },
        addReleaseToQueue: @escaping @Sendable (String) -> Void = { _ in },
        addReleaseNext: @escaping @Sendable (String) -> Void = { _ in },
        insertInQueue: @escaping @Sendable ([String], UInt32) -> Void = {
            _,
            _ in
        },
        removeEntry: @escaping @Sendable (String) -> Void = { _ in },
        clearQueue: @escaping @Sendable () -> Void = {},
        reorderEntry: @escaping @Sendable (String, String?) -> Void = { _, _ in
        },
        skipToEntry: @escaping @Sendable (String) -> Void = { _ in },
        setShuffle: @escaping @Sendable (Bool) -> Void = { _ in },
        getUpcomingPage:
            @escaping @Sendable (UInt32, UInt32) async throws ->
            BridgeQueueUpcomingPage = { _, _ in
                BridgeQueueUpcomingPage(revision: 0, entries: [])
            }
    ) {
        self.addToQueue = addToQueue
        self.addNext = addNext
        self.addReleaseToQueue = addReleaseToQueue
        self.addReleaseNext = addReleaseNext
        self.insertInQueue = insertInQueue
        self.removeEntry = removeEntry
        self.clearQueue = clearQueue
        self.reorderEntry = reorderEntry
        self.skipToEntry = skipToEntry
        self.setShuffle = setShuffle
        self.getUpcomingPage = getUpcomingPage
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            addToQueue: { handle.addToQueue(trackIds: $0) },
            addNext: { handle.addNext(trackIds: $0) },
            addReleaseToQueue: { handle.addReleaseToQueue(releaseId: $0) },
            addReleaseNext: { handle.addReleaseNext(releaseId: $0) },
            insertInQueue: { handle.insertInQueue(trackIds: $0, index: $1) },
            removeEntry: { handle.removeEntry(entryId: $0) },
            clearQueue: { handle.clearQueue() },
            reorderEntry: {
                handle.reorderEntry(entryId: $0, beforeEntryId: $1)
            },
            skipToEntry: { handle.skipToEntry(entryId: $0) },
            setShuffle: { handle.setShuffle(on: $0) },
            getUpcomingPage: {
                try await handle.getQueueUpcomingPage(offset: $0, limit: $1)
            }
        )
    }

    // periphery:ignore
    public static let stub = Queue()
}
