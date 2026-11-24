import Foundation

/// The threads picture decodes run on: a dispatch queue of the app's own that
/// runs at most `width` decodes at once.
///
/// A decode blocks its thread: ImageIO reads the file through a memory
/// mapping, so a file on a network share makes the thread wait on the network
/// inside a page fault. Swift's cooperative pool has one thread per core, so
/// decodes that ran there would starve every other piece of async work in the
/// app while they waited. Each decode runs here instead, and its caller awaits
/// it through a continuation.
public final class ImageDecodeQueue: @unchecked Sendable {
    /// The width of a queue made with `init()`: at most four decodes, and at
    /// least two, so one slow file never holds up every other picture.
    public static var defaultWidth: Int {
        max(2, min(4, ProcessInfo.processInfo.activeProcessorCount))
    }

    /// One request for a decode, from the moment it is submitted.
    private final class Entry: @unchecked Sendable {
        /// Runs on a decode thread: starts the work if the request still
        /// wants it, and resumes the caller.
        let execute: @Sendable () -> Void
        /// Resumes the caller of a request that never started.
        let abandon: @Sendable () -> Void

        init(
            execute: @escaping @Sendable () -> Void,
            abandon: @escaping @Sendable () -> Void
        ) {
            self.execute = execute
            self.abandon = abandon
        }
    }

    /// A caller's handle on its request, which the caller's cancellation
    /// reaches before the request has an entry. Guarded by the queue's lock.
    private final class Ticket: @unchecked Sendable {
        var cancelled = false
        var entry: Entry?
    }

    private let width: Int
    private let queue: DispatchQueue
    private let currentKey = DispatchSpecificKey<Void>()
    private let lock = NSLock()
    /// Requests that wait for a free thread, oldest first. Guarded by `lock`.
    private var pending: [Entry] = []
    /// Decodes under way. Guarded by `lock`.
    private var running = 0

    #if DEBUG
        /// Where one decode ran.
        public struct Run: Equatable, Sendable {
            /// On this queue's own threads.
            public let onQueue: Bool
            public let onMainThread: Bool
        }

        private var recordedRuns: [Run] = []

        /// Runs on the decode thread just before each started decode, for a
        /// test to hold a decode while it is under way. Set it before the
        /// first request.
        public var beforeRun: (@Sendable () -> Void)?

        /// Every decode this queue has started, in the order they started.
        public var runs: [Run] {
            lock.lock()
            defer { lock.unlock() }
            return recordedRuns
        }
    #endif

    public convenience init() {
        self.init(width: Self.defaultWidth)
    }

    public init(width: Int) {
        precondition(width > 0, "a decode queue runs at least one decode")
        self.width = width
        queue = DispatchQueue(
            label: "fm.bae.image-decode",
            qos: .userInitiated,
            attributes: .concurrent
        )
        queue.setSpecific(key: currentKey, value: ())
    }

    /// Run `work` on this queue and return what it returns. Throws
    /// `CancellationError` when the surrounding task is cancelled before the
    /// work starts, or when `shouldStart` answers false at the moment a thread
    /// is free for it. Work that has started runs to its end, and its result
    /// is returned even to a cancelled caller: a decode can't be interrupted,
    /// and its result is worth keeping.
    public func run<T: Sendable>(
        shouldStart: @escaping @Sendable () -> Bool = { true },
        _ work: @escaping @Sendable () -> T
    ) async throws -> T {
        let ticket = Ticket()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let entry = Entry(
                    execute: { [self] in
                        guard shouldStart() else {
                            continuation.resume(throwing: CancellationError())
                            return
                        }
                        recordRun()
                        #if DEBUG
                            beforeRun?()
                        #endif
                        continuation.resume(returning: work())
                    },
                    abandon: {
                        continuation.resume(throwing: CancellationError())
                    }
                )
                submit(entry, for: ticket)
            }
        } onCancel: {
            cancel(ticket)
        }
    }

    private func submit(_ entry: Entry, for ticket: Ticket) {
        lock.lock()
        if ticket.cancelled {
            lock.unlock()
            entry.abandon()
            return
        }
        ticket.entry = entry
        pending.append(entry)
        lock.unlock()
        startWaiting()
    }

    private func cancel(_ ticket: Ticket) {
        lock.lock()
        ticket.cancelled = true
        var abandoned: Entry?
        if let entry = ticket.entry,
            let index = pending.firstIndex(where: { $0 === entry })
        {
            pending.remove(at: index)
            abandoned = entry
        }
        lock.unlock()
        abandoned?.abandon()
    }

    /// Start waiting requests while a thread is free.
    private func startWaiting() {
        while let entry = takeWaiting() {
            queue.async { [self] in
                entry.execute()
                finished()
            }
        }
    }

    private func takeWaiting() -> Entry? {
        lock.lock()
        defer { lock.unlock() }
        guard running < width, !pending.isEmpty else { return nil }
        running += 1
        return pending.removeFirst()
    }

    private func finished() {
        lock.lock()
        running -= 1
        lock.unlock()
        startWaiting()
    }

    private func recordRun() {
        #if DEBUG
            let run = Run(
                onQueue: DispatchQueue.getSpecific(key: currentKey) != nil,
                onMainThread: Thread.isMainThread
            )
            lock.lock()
            recordedRuns.append(run)
            lock.unlock()
        #endif
    }
}
