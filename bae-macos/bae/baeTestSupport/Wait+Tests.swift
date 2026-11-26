import AppKit
import Foundation
import ObjectiveC

/// What bae's tests wait for beyond what the shots harness's `Wait` does.
extension Wait {
    /// [`until`] for a condition that reads an actor, which observation
    /// tracking cannot follow: it is read again after every `happened()`.
    @MainActor
    static func until(
        notices: [Notice] = [],
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () async throws -> Bool
    ) async throws {
        let listening = Listening(notices)
        defer { listening.stop() }
        let end = ContinuousClock.now.advanced(by: HangGuard.duration)
        while true {
            let wake = Wake()
            let registration = Wake.register(wake)
            defer { Wake.unregister(registration) }
            if try await condition() { return }
            try await wake.fired(
                before: end,
                file: file,
                line: line
            )
        }
    }
}

extension Wait.Notice {
    /// A change of `keyPath` on `object`, as key-value observing reports.
    init<Object: NSObject, Value>(
        changesOf keyPath: KeyPath<Object, Value>,
        on object: Object
    ) {
        listen = { fire in
            let observation = object.observe(keyPath, options: []) { _, _ in
                fire()
            }
            return Wait.Listener { observation.invalidate() }
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
