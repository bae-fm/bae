import Foundation

private final class QueueUpcomingValueSink: QueueUpcomingCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeQueueUpcomingPage) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeQueueUpcomingPage) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeQueueUpcomingPage) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
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
    /// Subscribe to one page of the context's upcoming tail past the initial
    /// window. `offset` 0 is the first not-yet-played entry after the current
    /// track.
    public let subscribeUpcomingPage:
        @Sendable (
            _ offset: UInt32, _ limit: UInt32,
            _ onValue:
                @escaping @MainActor @Sendable (BridgeQueueUpcomingPage) -> Void,
            _ onError: @escaping @MainActor @Sendable (any Error) -> Void
        ) -> any LiveSubscriptionProtocol

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
        subscribeUpcomingPage:
            @escaping @Sendable (
                UInt32, UInt32,
                @escaping @MainActor @Sendable (BridgeQueueUpcomingPage) -> Void,
                @escaping @MainActor @Sendable (any Error) -> Void
            ) -> any LiveSubscriptionProtocol = { _, _, _, _ in
                fatalError("Queue upcoming-page subscription is not installed")
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
        self.subscribeUpcomingPage = subscribeUpcomingPage
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
            subscribeUpcomingPage: { offset, limit, onValue, onError in
                handle.subscribeQueueUpcomingPage(
                    offset: offset,
                    limit: limit,
                    callback: QueueUpcomingValueSink(
                        apply: onValue,
                        fail: onError
                    )
                )
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Queue { Queue() }
    #endif
}
