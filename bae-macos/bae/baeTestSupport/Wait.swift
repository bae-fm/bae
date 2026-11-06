import AppKit
import Foundation
import ObjectiveC

/// Waiting in a test for something that happens later: a value landing in a
/// store, a recorder receiving a call, a hosted view finishing its layout.
///
/// A wait ends on the event it waits for, never on a clock. A store's value is
/// `@Observable`, so the condition is read under observation tracking and the
/// wait resumes when anything the condition read changes. A recorder or a fake
/// is test code: it calls `Wait.happened()` after each change it records, which
/// resumes every wait to read its condition again. A hosted view tree is read
/// each time the main run loop goes idle, about to sleep. The only timer is
/// the deadline, which fails the wait loudly when the event never comes.
enum Wait {
    /// What a wait was given did not happen in time.
    struct TimedOut: LocalizedError, CustomStringConvertible {
        let after: Duration
        let file: StaticString
        let line: UInt

        var description: String {
            "the wait at \(file):\(line) was still waiting after \(after)"
        }

        var errorDescription: String? { description }
    }

    /// How long any one wait holds out. Far past anything a passing test
    /// needs, so reaching it means the thing waited for is not coming.
    static let deadline: Duration = .seconds(10)

    /// Return once `condition` holds. It is read under observation tracking
    /// and again after every `happened()`, so it holds only things a store
    /// publishes or a recorder announces.
    ///
    /// What AppKit changes without observation, such as a clip view's bounds,
    /// is named in `notices`: each posting of one reads the condition again.
    @MainActor
    static func until(
        notices: [Notice] = [],
        timeout: Duration = deadline,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () throws -> Bool
    ) async throws {
        let listening = Listening(notices)
        defer { listening.stop() }
        let end = ContinuousClock.now.advanced(by: timeout)
        while true {
            let wake = Wake()
            let registration = Wake.register(wake)
            defer { Wake.unregister(registration) }
            var holds = false
            var failure: (any Error)?
            withObservationTracking {
                do { holds = try condition() }
                catch { failure = error }
            } onChange: {
                wake.fire()
            }
            if let failure { throw failure }
            if holds { return }
            try await wake.fired(
                before: end,
                timeout: timeout,
                file: file,
                line: line
            )
        }
    }

    /// [`until`] for a condition that reads an actor, which observation
    /// tracking cannot follow: it is read again after every `happened()`.
    @MainActor
    static func until(
        notices: [Notice] = [],
        timeout: Duration = deadline,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () async throws -> Bool
    ) async throws {
        let listening = Listening(notices)
        defer { listening.stop() }
        let end = ContinuousClock.now.advanced(by: timeout)
        while true {
            let wake = Wake()
            let registration = Wake.register(wake)
            defer { Wake.unregister(registration) }
            if try await condition() { return }
            try await wake.fired(
                before: end,
                timeout: timeout,
                file: file,
                line: line
            )
        }
    }

    /// Something a test waits on changed: a recorder took a call, a fake
    /// was asked. Call it after the change is visible to a reader, from any
    /// thread.
    static func happened() {
        Wake.fireAll()
    }

    /// Something AppKit changes without observation, which a wait reads its
    /// condition again on: a notification, or a key-value-observed property.
    struct Notice {
        fileprivate let listen: (@escaping @Sendable () -> Void) -> Listener

        init(_ name: Notification.Name, of object: AnyObject?) {
            listen = { fire in
                Listener(
                    NotificationCenter.default.addObserver(
                        forName: name,
                        object: object,
                        queue: nil
                    ) { _ in fire() }
                )
            }
        }

        /// A change of `keyPath` on `object`, as key-value observing reports.
        init<Object: NSObject, Value>(
            changesOf keyPath: KeyPath<Object, Value>,
            on object: Object
        ) {
            listen = { fire in
                Listener(
                    object.observe(keyPath, options: []) { _, _ in fire() }
                )
            }
        }
    }

    fileprivate final class Listener: @unchecked Sendable {
        private let stopping: () -> Void

        init(_ observer: any NSObjectProtocol) {
            stopping = { NotificationCenter.default.removeObserver(observer) }
        }

        init(_ observation: NSKeyValueObservation) {
            stopping = { observation.invalidate() }
        }

        func stop() { stopping() }
    }

    /// The observers of a wait's notices, for as long as it runs.
    private final class Listening: @unchecked Sendable {
        private var listeners: [Listener]

        init(_ notices: [Notice]) {
            listeners = notices.map { $0.listen { Wait.happened() } }
        }

        func stop() {
            for listener in listeners { listener.stop() }
            listeners = []
        }
    }

    /// One wait's wake-up: fired by the observation of what it read or by
    /// `happened()`. A fire before the wait began is kept.
    private final class Wake: @unchecked Sendable {
        private let lock = NSLock()
        private var isFired = false
        private var waiter: CheckedContinuation<Void, Never>?

        private static let registryLock = NSLock()
        nonisolated(unsafe) private static var registry:
            [ObjectIdentifier: Wake] = [:]

        static func register(_ wake: Wake) -> ObjectIdentifier {
            let id = ObjectIdentifier(wake)
            registryLock.withLock { registry[id] = wake }
            return id
        }

        static func unregister(_ id: ObjectIdentifier) {
            registryLock.withLock { _ = registry.removeValue(forKey: id) }
        }

        static func fireAll() {
            let all = registryLock.withLock { Array(registry.values) }
            for wake in all { wake.fire() }
        }

        func fire() {
            let resume: CheckedContinuation<Void, Never>? = lock.withLock {
                isFired = true
                defer { waiter = nil }
                return waiter
            }
            resume?.resume()
        }

        private func wait() async {
            await withCheckedContinuation { continuation in
                let resumeNow = lock.withLock {
                    if isFired { return true }
                    waiter = continuation
                    return false
                }
                if resumeNow { continuation.resume() }
            }
        }

        /// Return when fired; throw when `end` comes first.
        func fired(
            before end: ContinuousClock.Instant,
            timeout: Duration,
            file: StaticString,
            line: UInt
        ) async throws {
            try await withThrowingTaskGroup(of: Void.self) { group in
                group.addTask {
                    await withTaskCancellationHandler {
                        await self.wait()
                    } onCancel: {
                        self.fire()
                    }
                }
                group.addTask {
                    try await Task.sleep(until: end, clock: ContinuousClock())
                    throw TimedOut(after: timeout, file: file, line: line)
                }
                defer { group.cancelAll() }
                try await group.next()
            }
        }
    }
}

extension NSLock {
    /// [`withLock`](NSLock/withLock(_:)) for a recorder's change: once the
    /// change is made and the lock let go, every wait reads its condition
    /// again.
    func recording<Result>(_ change: () throws -> Result) rethrows -> Result {
        let result = try withLock(change)
        Wait.happened()
        return result
    }
}

extension Wait.Notice {
    /// The table's selection changed.
    static func selection(of table: NSTableView) -> Wait.Notice {
        Wait.Notice(NSTableView.selectionDidChangeNotification, of: table)
    }

    /// The table's scroll view scrolled.
    static func scroll(of table: NSTableView) -> Wait.Notice {
        Wait.Notice(
            NSView.boundsDidChangeNotification,
            of: table.enclosingScrollView?.contentView
        )
    }
}

extension Wait {
    /// Tell every wait when `object` is gone: a wait for a weak reference to
    /// read `nil` reads it again once the object has deallocated. The
    /// announcer lives on the object, so it deinitializes with it.
    static func announceRelease(of object: AnyObject) {
        objc_setAssociatedObject(
            object,
            &ReleaseAnnouncer.key,
            ReleaseAnnouncer(),
            .OBJC_ASSOCIATION_RETAIN_NONATOMIC
        )
    }

    private final class ReleaseAnnouncer {
        nonisolated(unsafe) static var key = 0

        deinit { Wait.happened() }
    }
}

extension [Wait.Notice] {
    /// The text of each of `fields` changed.
    @MainActor
    static func text(of fields: [NSTextField]) -> [Wait.Notice] {
        fields.map { Wait.Notice(changesOf: \.stringValue, on: $0) }
    }
}

extension Wait {
    /// Return once `fields` read `expected`, as each field's text changes.
    @MainActor
    static func untilText(
        of fields: [NSTextField],
        is expected: [String],
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        try await until(
            notices: .text(of: fields),
            file: file,
            line: line
        ) {
            fields.map(\.stringValue) == expected
        }
    }
}

// Waits for a hosted view tree, read at each idle of the main run loop.
extension Wait {
    /// Return once `condition` holds of a hosted view tree, laid out before
    /// each reading: at once, or at a later idle of the main run loop.
    /// SwiftUI publishes a tree's changes over several main-actor turns,
    /// each ending when the run loop has nothing left to run, so the tree is
    /// read again at each idle. Whatever a store or a recorder holds is
    /// waited for with [`until`].
    @MainActor
    static func untilLaidOut(
        _ host: NSView,
        timeout: Duration = deadline,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () throws -> Bool
    ) async throws {
        let end = ContinuousClock.now.advanced(by: timeout)
        while true {
            host.layoutSubtreeIfNeeded()
            if try condition() { return }
            try await nextIdle(
                before: end,
                timeout: timeout,
                file: file,
                line: line
            )
        }
    }

    /// Return at the main run loop's next idle: once it has run SwiftUI's
    /// and Core Animation's updates and is about to sleep.
    ///
    /// An idle is not the end of all main-actor work: one can come while
    /// main-actor work is still queued, and the run loop wakes again at once
    /// to run it: a storage manager's view asked for its list three idles
    /// after its window opened, its tree unchanged across them. Work running
    /// off the main thread is not waited for either: what it brings back
    /// wakes the run loop, and its own event is what a wait for it reads.
    @MainActor
    static func nextIdle(
        before end: ContinuousClock.Instant,
        timeout: Duration,
        file: StaticString,
        line: UInt
    ) async throws {
        let wake = Wake()
        // Ordered last, so every other observer of the same pass has run.
        let observer = CFRunLoopObserverCreateWithHandler(
            nil,
            CFRunLoopActivity.beforeWaiting.rawValue,
            false,
            CFIndex.max
        ) { _, _ in wake.fire() }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        defer { CFRunLoopObserverInvalidate(observer) }
        try await wake.fired(
            before: end,
            timeout: timeout,
            file: file,
            line: line
        )
    }

    /// Return once `value` reads the same at two idles of the main run loop
    /// in a row: a hosted tree whose frames stopped moving.
    ///
    /// SwiftUI lays a hosted tree out over several main-actor turns, each
    /// ending at an idle of the run loop: a geometry reader publishes a
    /// width, the views under it measure again, work a view started on the
    /// main actor lands. A tree read the same across an idle has none of
    /// that left, unless what is left is a chain of main-actor steps that
    /// moves no frame, since an idle can come with main-actor work still
    /// queued. Such a chain, and work off the main thread such as a page a
    /// query answers, is waited for by its own event first.
    ///
    /// A tree that has not started moving yet also reads the same, so when
    /// the step before the wait must change the value — a scroll — the
    /// caller passes what it read before that step as `changedFrom`, and the
    /// wait holds out for a reading that differs from it first.
    @MainActor
    static func untilSteady<Value: Equatable>(
        changedFrom before: Value? = nil,
        timeout: Duration = deadline,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ value: @MainActor () async throws -> Value
    ) async throws {
        let end = ContinuousClock.now.advanced(by: timeout)
        var last = try await value()
        var changed = true
        if let before { changed = before != last }
        while true {
            try await nextIdle(
                before: end,
                timeout: timeout,
                file: file,
                line: line
            )
            let now = try await value()
            if now == last, changed { return }
            last = now
            if let before, !changed { changed = before != now }
        }
    }
}
