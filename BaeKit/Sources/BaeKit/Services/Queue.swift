import Foundation

/// The context's upcoming tail, read through one live subscription: which
/// windows it reads is changed in place, and each value answers every window
/// at once with the tail's length, and says whether the queue changed or only
/// the windows moved.
public struct QueueUpcomingQuery: Sendable {
    public let setWindows: @Sendable ([BridgeQueueWindow]) throws -> Void
    public let values: PulledValues<BridgeQueueUpcomingSnapshot>

    public init(
        setWindows:
            @escaping @Sendable ([BridgeQueueWindow]) throws -> Void,
        values: PulledValues<BridgeQueueUpcomingSnapshot>
    ) {
        self.setWindows = setWindows
        self.values = values
    }
}

/// Queue mutations — appending, inserting, reordering, removing,
/// jumping to a queue entry. Reorder/remove/skip target one row by its
/// `entryId`. Narrow subset of `AppHandle`.
public final class Queue: Sendable, Observable {
    public let addToQueue: @Sendable (_ trackIds: [String]) -> Void
    public let addNext: @Sendable (_ trackIds: [String]) -> Void
    public let insertInQueue:
        @Sendable (_ trackIds: [String], _ index: UInt32) -> Void
    /// Remove the row `entryId`. A row the queue no longer holds is already
    /// gone, and this answers as if it had removed it.
    public let removeEntry: @Sendable (_ entryId: String) async throws -> Void
    /// Empty the manual lane, leaving the context lane playing.
    public let clearUpNext: @Sendable () async throws -> Void
    /// Drop the context lane. The playing track keeps playing.
    public let clearPlayingFrom: @Sendable () async throws -> Void
    /// Move `entryId` to sit before `beforeEntryId`; `nil` moves it to the end
    /// of its lane. Answers the lane's version the move made, or `nil` when the
    /// queue changed first and core moved nothing.
    public let reorderEntry:
        @Sendable (_ entryId: String, _ beforeEntryId: String?) async throws
            -> UInt64?
    /// Skip to the row `entryId`. A row the queue no longer holds is not
    /// skipped to, and this answers as if it had been.
    public let skipToEntry: @Sendable (_ entryId: String) async throws -> Void
    /// Flip the playing context between sequential and shuffled order.
    public let setShuffle: @Sendable (_ on: Bool) -> Void
    /// Open the live read of the context's upcoming tail. It reads no
    /// windows until its first `setWindows`.
    public let subscribeUpcoming: @Sendable () -> QueueUpcomingQuery

    public init(
        addToQueue: @escaping @Sendable ([String]) -> Void = { _ in },
        addNext: @escaping @Sendable ([String]) -> Void = { _ in },
        insertInQueue: @escaping @Sendable ([String], UInt32) -> Void = {
            _,
            _ in
        },
        removeEntry: @escaping @Sendable (String) async throws -> Void = { _ in
        },
        clearUpNext: @escaping @Sendable () async throws -> Void = {},
        clearPlayingFrom: @escaping @Sendable () async throws -> Void = {},
        reorderEntry:
            @escaping @Sendable (String, String?) async throws -> UInt64? = {
                _,
                _ in 0
            },
        skipToEntry: @escaping @Sendable (String) async throws -> Void = { _ in
        },
        setShuffle: @escaping @Sendable (Bool) -> Void = { _ in },
        subscribeUpcoming: @escaping @Sendable () -> QueueUpcomingQuery = {
            fatalError("Queue upcoming subscription is not installed")
        }
    ) {
        self.addToQueue = addToQueue
        self.addNext = addNext
        self.insertInQueue = insertInQueue
        self.removeEntry = removeEntry
        self.clearUpNext = clearUpNext
        self.clearPlayingFrom = clearPlayingFrom
        self.reorderEntry = reorderEntry
        self.skipToEntry = skipToEntry
        self.setShuffle = setShuffle
        self.subscribeUpcoming = subscribeUpcoming
    }

    /// Queue commands over `handle`. An add, an insert or a shuffle has no
    /// caller waiting on it: why one failed goes to `show`.
    public convenience init(
        handle: any AppHandleProtocol,
        show: @escaping @MainActor @Sendable (DisplayError) -> Void
    ) {
        self.init(
            addToQueue: { trackIds in
                answerCommand(show) {
                    try await handle.addToQueue(trackIds: trackIds)
                }
            },
            addNext: { trackIds in
                answerCommand(show) {
                    try await handle.addNext(trackIds: trackIds)
                }
            },
            insertInQueue: { trackIds, index in
                answerCommand(show) {
                    try await handle.insertInQueue(
                        trackIds: trackIds,
                        index: index
                    )
                }
            },
            removeEntry: { try await handle.removeEntry(entryId: $0) },
            clearUpNext: { try await handle.clearUpNext() },
            clearPlayingFrom: { try await handle.clearPlayingFrom() },
            reorderEntry: {
                try await handle.reorderEntry(entryId: $0, beforeEntryId: $1)
            },
            skipToEntry: { try await handle.skipToEntry(entryId: $0) },
            setShuffle: { on in
                answerCommand(show) { try await handle.setShuffle(on: on) }
            },
            subscribeUpcoming: {
                let subscription = handle.subscribeQueueUpcoming()
                return QueueUpcomingQuery(
                    setWindows: { try subscription.setWindows(windows: $0) },
                    values: PulledValues(subscription)
                )
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Queue { Queue() }
    #endif
}
