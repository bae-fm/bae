import Foundation

/// The context's upcoming tail past the queue snapshot's first window, read
/// through one live subscription: which windows it reads is changed in place,
/// and each value answers every window at once, stamped with the queue revision
/// it was sliced from.
public struct QueueUpcomingQuery: Sendable {
    public let setWindows: @Sendable ([BridgeLibraryPageWindow]) throws -> Void
    public let next: @Sendable () async throws -> BridgeQueueUpcomingSnapshot
    public let cancel: @Sendable () async -> Void

    public init(
        setWindows:
            @escaping @Sendable ([BridgeLibraryPageWindow]) throws -> Void,
        next:
            @escaping @Sendable () async throws -> BridgeQueueUpcomingSnapshot,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setWindows = setWindows
        self.next = next
        self.cancel = cancel
    }
}

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
    /// Empty the manual lane, leaving the context lane playing.
    public let clearUpNext: @Sendable () -> Void
    /// Drop the context lane. The playing track keeps playing.
    public let clearPlayingFrom: @Sendable () -> Void
    /// Move `entryId` to sit before `beforeEntryId`; `nil` moves it to the end.
    public let reorderEntry:
        @Sendable (_ entryId: String, _ beforeEntryId: String?) -> Void
    public let skipToEntry: @Sendable (_ entryId: String) -> Void
    /// Flip the playing context between sequential and shuffled order.
    public let setShuffle: @Sendable (_ on: Bool) -> Void
    /// Open the live read of the context's upcoming tail. It reads no
    /// windows until its first `setWindows`.
    public let subscribeUpcoming: @Sendable () -> QueueUpcomingQuery

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
        clearUpNext: @escaping @Sendable () -> Void = {},
        clearPlayingFrom: @escaping @Sendable () -> Void = {},
        reorderEntry: @escaping @Sendable (String, String?) -> Void = { _, _ in
        },
        skipToEntry: @escaping @Sendable (String) -> Void = { _ in },
        setShuffle: @escaping @Sendable (Bool) -> Void = { _ in },
        subscribeUpcoming: @escaping @Sendable () -> QueueUpcomingQuery = {
            fatalError("Queue upcoming subscription is not installed")
        }
    ) {
        self.addToQueue = addToQueue
        self.addNext = addNext
        self.addReleaseToQueue = addReleaseToQueue
        self.addReleaseNext = addReleaseNext
        self.insertInQueue = insertInQueue
        self.removeEntry = removeEntry
        self.clearUpNext = clearUpNext
        self.clearPlayingFrom = clearPlayingFrom
        self.reorderEntry = reorderEntry
        self.skipToEntry = skipToEntry
        self.setShuffle = setShuffle
        self.subscribeUpcoming = subscribeUpcoming
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            addToQueue: { handle.addToQueue(trackIds: $0) },
            addNext: { handle.addNext(trackIds: $0) },
            addReleaseToQueue: { handle.addReleaseToQueue(releaseId: $0) },
            addReleaseNext: { handle.addReleaseNext(releaseId: $0) },
            insertInQueue: { handle.insertInQueue(trackIds: $0, index: $1) },
            removeEntry: { handle.removeEntry(entryId: $0) },
            clearUpNext: { handle.clearUpNext() },
            clearPlayingFrom: { handle.clearPlayingFrom() },
            reorderEntry: {
                handle.reorderEntry(entryId: $0, beforeEntryId: $1)
            },
            skipToEntry: { handle.skipToEntry(entryId: $0) },
            setShuffle: { handle.setShuffle(on: $0) },
            subscribeUpcoming: {
                let subscription = handle.subscribeQueueUpcoming()
                return QueueUpcomingQuery(
                    setWindows: { try subscription.setWindows(windows: $0) },
                    next: { try await subscription.next() },
                    cancel: { try? await subscription.cancel() }
                )
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Queue { Queue() }
    #endif
}
