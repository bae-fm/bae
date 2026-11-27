import AppKit
import BaeKit
import Foundation
import ObjectiveC

/// What bae's tests wait for beyond what the shots harness's `Wait` does.
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
    /// Whether `object` is gone, which a wait for its release reads. The
    /// announcer lives on the object, so it deinitializes with it.
    static func release(of object: AnyObject) -> Recorded<Bool> {
        let released = Recorded(false)
        objc_setAssociatedObject(
            object,
            &ReleaseAnnouncer.key,
            ReleaseAnnouncer(released),
            .OBJC_ASSOCIATION_RETAIN_NONATOMIC
        )
        return released
    }

    private final class ReleaseAnnouncer {
        nonisolated(unsafe) static var key = 0

        private let released: Recorded<Bool>

        init(_ released: Recorded<Bool>) {
            self.released = released
        }

        deinit { released.change { $0 = true } }
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
