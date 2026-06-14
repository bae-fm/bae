import Foundation

/// Queue mutations — appending, inserting, reordering, removing,
/// jumping to a queue index. Narrow subset of `AppHandle`.
final class Queue: Sendable, Observable {
    let addToQueue: @Sendable (_ trackIds: [String]) -> Void
    let addNext: @Sendable (_ trackIds: [String]) -> Void
    let addReleaseToQueue: @Sendable (_ releaseId: String) -> Void
    let addReleaseNext: @Sendable (_ releaseId: String) -> Void
    let insertInQueue: @Sendable (_ trackIds: [String], _ index: UInt32) -> Void
    let removeFromQueue: @Sendable (_ index: UInt32) -> Void
    let clearQueue: @Sendable () -> Void
    let reorderQueue: @Sendable (_ fromIndex: UInt32, _ toIndex: UInt32) -> Void
    let skipToQueueIndex: @Sendable (_ index: UInt32) -> Void

    init(
        addToQueue: @escaping @Sendable ([String]) -> Void = { _ in },
        addNext: @escaping @Sendable ([String]) -> Void = { _ in },
        addReleaseToQueue: @escaping @Sendable (String) -> Void = { _ in },
        addReleaseNext: @escaping @Sendable (String) -> Void = { _ in },
        insertInQueue: @escaping @Sendable ([String], UInt32) -> Void = {
            _,
            _ in
        },
        removeFromQueue: @escaping @Sendable (UInt32) -> Void = { _ in },
        clearQueue: @escaping @Sendable () -> Void = {},
        reorderQueue: @escaping @Sendable (UInt32, UInt32) -> Void = { _, _ in
        },
        skipToQueueIndex: @escaping @Sendable (UInt32) -> Void = { _ in }
    ) {
        self.addToQueue = addToQueue
        self.addNext = addNext
        self.addReleaseToQueue = addReleaseToQueue
        self.addReleaseNext = addReleaseNext
        self.insertInQueue = insertInQueue
        self.removeFromQueue = removeFromQueue
        self.clearQueue = clearQueue
        self.reorderQueue = reorderQueue
        self.skipToQueueIndex = skipToQueueIndex
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            addToQueue: { handle.addToQueue(trackIds: $0) },
            addNext: { handle.addNext(trackIds: $0) },
            addReleaseToQueue: { handle.addReleaseToQueue(releaseId: $0) },
            addReleaseNext: { handle.addReleaseNext(releaseId: $0) },
            insertInQueue: { handle.insertInQueue(trackIds: $0, index: $1) },
            removeFromQueue: { handle.removeFromQueue(index: $0) },
            clearQueue: { handle.clearQueue() },
            reorderQueue: { handle.reorderQueue(fromIndex: $0, toIndex: $1) },
            skipToQueueIndex: { handle.skipToQueueIndex(index: $0) }
        )
    }

    // periphery:ignore
    static let stub = Queue()
}
