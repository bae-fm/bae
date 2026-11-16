import Foundation

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
    /// Move `entryId` to `position` among the rows `lane` shows, or to the
    /// lane's end past them; core finds the row it then sits before. Answers
    /// whether it moved: `false` when the entry is no longer in `lane` and
    /// core moved nothing.
    public let reorderEntry:
        @Sendable (
            _ entryId: String, _ lane: BridgeQueueLane, _ position: UInt64
        ) async throws -> Bool
    /// Skip to the row `entryId`. A row the queue no longer holds is not
    /// skipped to, and this answers as if it had been.
    public let skipToEntry: @Sendable (_ entryId: String) async throws -> Void
    /// Flip the playing context between sequential and shuffled order.
    public let setShuffle: @Sendable (_ on: Bool) -> Void
    /// Opens the live queries of the context's upcoming tracks, in the lane's
    /// own order, or shuffled when `shuffled` is true.
    public let upcomingQueries:
        @Sendable (_ shuffled: Bool) -> BridgeListQueries<BridgeQueueEntry>

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
            @escaping @Sendable (String, BridgeQueueLane, UInt64) async throws
            -> Bool = { _, _, _ in true },
        skipToEntry: @escaping @Sendable (String) async throws -> Void = { _ in
        },
        setShuffle: @escaping @Sendable (Bool) -> Void = { _ in },
        upcomingQueries:
            @escaping @Sendable (Bool) -> BridgeListQueries<BridgeQueueEntry> =
            {
                _ in
                fatalError("Queue upcoming list is not installed")
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
        self.upcomingQueries = upcomingQueries
    }

    /// Queue commands over `handle`. An add, an insert or a shuffle has no
    /// caller waiting on it: why one failed goes to `show`, and how many
    /// tracks an add or an insert put in goes to `confirmAdded`, which the
    /// queue button's badge shows.
    public convenience init(
        handle: any AppHandleProtocol,
        show: @escaping @MainActor @Sendable (DisplayError) -> Void,
        confirmAdded: @escaping @MainActor @Sendable (Int) -> Void
    ) {
        self.init(
            addToQueue: { trackIds in
                answerCommand(show) {
                    try await handle.addToQueue(trackIds: trackIds)
                } confirm: {
                    confirmAdded(Int($0))
                }
            },
            addNext: { trackIds in
                answerCommand(show) {
                    try await handle.addNext(trackIds: trackIds)
                } confirm: {
                    confirmAdded(Int($0))
                }
            },
            insertInQueue: { trackIds, index in
                answerCommand(show) {
                    try await handle.insertInQueue(
                        trackIds: trackIds,
                        index: index
                    )
                } confirm: {
                    confirmAdded(Int($0))
                }
            },
            removeEntry: { try await handle.removeEntry(entryId: $0) },
            clearUpNext: { try await handle.clearUpNext() },
            clearPlayingFrom: { try await handle.clearPlayingFrom() },
            reorderEntry: {
                try await handle.reorderEntry(
                    entryId: $0,
                    lane: $1,
                    position: $2
                )
            },
            skipToEntry: { try await handle.skipToEntry(entryId: $0) },
            setShuffle: { on in
                answerCommand(show) { try await handle.setShuffle(on: on) }
            },
            upcomingQueries: Self.upcomingQueries(handle)
        )
    }

    #if os(macOS)
        /// The upcoming tracks' list under each order, read through `handle`.
        private static func upcomingQueries(
            _ handle: any AppHandleProtocol
        ) -> @Sendable (Bool) -> BridgeListQueries<BridgeQueueEntry> {
            { shuffled in
                BridgeListQueries(
                    handle: handle,
                    list: .upcoming(shuffled: shuffled),
                    rows: { rows in
                        guard case .upcoming(let entries) = rows else {
                            preconditionFailure(
                                "the upcoming list delivered rows of another list"
                            )
                        }
                        return entries
                    }
                )
            }
        }
    #else
        private static func upcomingQueries(
            _ handle: any AppHandleProtocol
        ) -> @Sendable (Bool) -> BridgeListQueries<BridgeQueueEntry> {
            { _ in
                fatalError("only the macOS app reads the queue's upcoming list")
            }
        }
    #endif

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Queue { Queue() }
    #endif
}
