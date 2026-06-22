import Foundation

/// Queue mutations — appending, inserting, reordering, removing,
/// jumping to a queue entry. Reorder/remove/skip target a per-instance
/// `entryId`. Narrow subset of `AppHandle`.
final class Queue: Sendable, Observable {
    let addToQueue: @Sendable (_ trackIds: [String]) -> Void
    let addNext: @Sendable (_ trackIds: [String]) -> Void
    let addReleaseToQueue: @Sendable (_ releaseId: String) -> Void
    let addReleaseNext: @Sendable (_ releaseId: String) -> Void
    let insertInQueue: @Sendable (_ trackIds: [String], _ index: UInt32) -> Void
    let removeEntry: @Sendable (_ entryId: String) -> Void
    let clearQueue: @Sendable () -> Void
    /// Move `entryId` to sit before `beforeEntryId`; `nil` moves it to the end.
    let reorderEntry:
        @Sendable (_ entryId: String, _ beforeEntryId: String?) -> Void
    let skipToEntry: @Sendable (_ entryId: String) -> Void

    init(
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
        skipToEntry: @escaping @Sendable (String) -> Void = { _ in }
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
    }

    convenience init(handle: any AppHandleProtocol) {
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
            skipToEntry: { handle.skipToEntry(entryId: $0) }
        )
    }

    // periphery:ignore
    static let stub = Queue()
}
